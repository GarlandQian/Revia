"""从 src-tauri/icons/Revia.icon 重新生成 src-tauri/icons 里的应用图标。

用法（仓库根目录）：python3 tools/export-app-icon.py

Revia.icon 是应用图标的源文件，用 Icon Composer（Xcode 自带）编辑。图案是“叶脉分支”：
叶片沿主脉分成深浅两半，叶脉画成 Git 分支线，每条分支末端是一个提交，叶尖附近较大的圆点是 HEAD。

1. 用 actool 把 Revia.icon 编译成 Assets.car。macOS 26 起，系统会按外观切换浅色、深色、透明
   和着色图标。这里预先编译，是因为 Tauri CLI 2.12 在打包时调用 actool 会报
   “Bad file descriptor”（tauri-apps/tauri#15315），手动调用则正常；
2. 用 Icon Composer 自带的 ictool 把 Revia.icon 渲染成 824×824（浅色外观），按 Apple 的
   macOS 图标网格居中放进 1024×1024 透明画布，加一层柔和投影；
3. 用 `pnpm tauri icon` 生成 PNG、icon.icns 和 icon.ico，供更早的 macOS、Windows 和窗口
   图标使用。只覆盖 src-tauri/icons 里已有的文件，不新增 Android、iOS 等用不到的尺寸。

需要 Xcode 26 或更高版本，以及 Python 的 Pillow。
"""
import shutil, subprocess, tempfile
from pathlib import Path
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri/icons"
SOURCE = ICONS / "Revia.icon"
ICTOOL = "/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool"

with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    # actool 要求输入文件名与 --app-icon 一致
    shutil.copytree(SOURCE, tmp / "Icon.icon")
    (tmp / "car").mkdir()
    subprocess.run(["xcrun", "actool", str(tmp / "Icon.icon"), "--compile", str(tmp / "car"),
                    "--output-format", "human-readable-text", "--errors",
                    "--output-partial-info-plist", str(tmp / "car/info.plist"),
                    "--app-icon", "Icon", "--include-all-app-icons", "--development-region", "en",
                    "--target-device", "mac", "--minimum-deployment-target", "26.0", "--platform", "macosx"],
                   stdin=subprocess.DEVNULL, check=True, capture_output=True)
    shutil.copyfile(tmp / "car/Assets.car", ICONS / "Assets.car")
    print((ICONS / "Assets.car").relative_to(ROOT))

    subprocess.run([ICTOOL, str(SOURCE), "--export-image", "--output-file", str(tmp / "body.png"),
                    "--platform", "macOS", "--rendition", "Default",
                    "--width", "824", "--height", "824", "--scale", "1"], check=True, capture_output=True)
    body = Image.open(tmp / "body.png").convert("RGBA")
    shadow = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 77), (100, 112), body.getchannel("A"))
    canvas = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    canvas.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(14)))
    canvas.alpha_composite(body, (100, 100))
    canvas.save(tmp / "app-icon.png")

    subprocess.run(["pnpm", "tauri", "icon", str(tmp / "app-icon.png"), "-o", str(tmp / "out")],
                   cwd=ROOT, check=True, capture_output=True)
    for existing in sorted(p for p in ICONS.iterdir() if p.suffix in {".png", ".icns", ".ico"}):
        shutil.copyfile(tmp / "out" / existing.name, existing)
        print(existing.relative_to(ROOT))
