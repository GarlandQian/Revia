//! Cocoa's terminate: bypasses Tauri's ExitRequested event (tao 0.37).
//! Keep the existing delegate/menu, cancel native termination, and let CloseDriver
//! own the request. Its final AppHandle::exit uses the runtime's stop: path.

use std::ffi::CString;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{Encode, MainThreadMarker, ffi, sel};
use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};

type ExitRequest = Box<dyn Fn() -> Result<()> + Send + Sync>;
static EXIT_REQUEST: OnceLock<ExitRequest> = OnceLock::new();

pub(super) fn install(app: &tauri::AppHandle) -> Result<()> {
    let main_thread = MainThreadMarker::new()
        .ok_or_else(|| AppError::internal("Native exit guard requires the main thread"))?;
    let application = NSApplication::sharedApplication(main_thread);
    let delegate = application
        .delegate()
        .ok_or_else(|| AppError::internal("Native application delegate is missing"))?;
    let delegate_object: &AnyObject = (*delegate).as_ref();
    let class = delegate_object.class();
    if class.name() != c"TaoAppDelegateParent" {
        return Err(AppError::internal(
            "Unsupported native application delegate",
        ));
    }
    ensure_unclaimed(class)?;
    let app = app.clone();
    EXIT_REQUEST
        .set(Box::new(move || {
            if let Some(window) = app.get_webview_window("main") {
                window.emit("revia://exit-requested", ()).map_err(|error| {
                    AppError::internal(format!("Native exit request delivery failed: {error}"))
                })
            } else {
                app.exit(0);
                Ok(())
            }
        }))
        .map_err(|_| AppError::internal("Native exit guard is already installed"))?;
    add_termination_method(class)?;

    // Refresh AppKit's optional-method cache without replacing Tao's delegate.
    application.setDelegate(None);
    application.setDelegate(Some(&delegate));
    Ok(())
}

fn ensure_unclaimed(class: &AnyClass) -> Result<()> {
    // class_addMethod alone can override an inherited method. Never do that.
    if class
        .instance_method(sel!(applicationShouldTerminate:))
        .is_some()
    {
        return Err(AppError::internal(
            "Native delegate already handles termination; review the adapter",
        ));
    }
    Ok(())
}

fn add_termination_method(class: &AnyClass) -> Result<()> {
    ensure_unclaimed(class)?;
    let encoding = CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))
        .map_err(|_| AppError::internal("Invalid native termination encoding"))?;
    let callback: unsafe extern "C-unwind" fn(
        &AnyObject,
        Sel,
        *mut AnyObject,
    ) -> NSApplicationTerminateReply = application_should_terminate;
    // The callback has Cocoa's NSUInteger return type and (self, selector, sender)
    // arguments. IMP erases that signature; the matching encoding restores it.
    let added = unsafe {
        let implementation: Imp = std::mem::transmute(callback);
        ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            sel!(applicationShouldTerminate:),
            implementation,
            encoding.as_ptr(),
        )
    };
    if !added.as_bool() {
        return Err(AppError::internal(
            "Cannot install native termination guard",
        ));
    }
    Ok(())
}

fn cancel_and_forward(request: impl FnOnce() -> Result<()>) -> NSApplicationTerminateReply {
    match catch_unwind(AssertUnwindSafe(request)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = writeln!(std::io::stderr(), "{error}");
        }
        Err(_) => {
            let _ = writeln!(
                std::io::stderr(),
                "Native exit forwarding panicked; termination cancelled"
            );
        }
    }
    // No NSTerminateLater/reply lifecycle: Wait/Stay belong to CloseDriver.
    NSApplicationTerminateReply::TerminateCancel
}

unsafe extern "C-unwind" fn application_should_terminate(
    _this: &AnyObject,
    _selector: Sel,
    _sender: *mut AnyObject,
) -> NSApplicationTerminateReply {
    cancel_and_forward(|| {
        EXIT_REQUEST
            .get()
            .ok_or_else(|| AppError::internal("Native exit handler is unavailable"))?()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::ClassBuilder;
    use std::cell::Cell;

    #[test]
    fn native_termination_never_exits_before_the_shared_driver_decides() {
        let calls = Cell::new(0);
        let reply = cancel_and_forward(|| {
            calls.set(calls.get() + 1);
            Ok(())
        });
        assert_eq!(calls.get(), 1);
        assert_eq!(reply, NSApplicationTerminateReply::TerminateCancel);
        assert_eq!(
            cancel_and_forward(|| Err(AppError::internal("fixture delivery failure"))),
            reply
        );
        assert_eq!(
            cancel_and_forward(|| panic!("fixture forwarding panic")),
            reply
        );
    }

    #[test]
    fn installed_selector_uses_cocoa_abi_and_rejects_replacement_or_inheritance() {
        let root = AnyClass::get(c"NSObject").unwrap();
        let class = ClassBuilder::new(c"ReviaTerminationBridgeTest", root)
            .unwrap()
            .register();
        add_termination_method(class).unwrap();
        assert!(add_termination_method(class).is_err());
        let child = ClassBuilder::new(c"ReviaTerminationBridgeChildTest", class)
            .unwrap()
            .register();
        assert!(add_termination_method(child).is_err());

        // Invoke the actual registered Objective-C selector, not a copied decision.
        let object: Retained<AnyObject> = unsafe { msg_send![class, new] };
        let reply: NSApplicationTerminateReply = unsafe {
            msg_send![&*object, applicationShouldTerminate: std::ptr::null_mut::<AnyObject>()]
        };
        assert_eq!(reply, NSApplicationTerminateReply::TerminateCancel);
    }
}
