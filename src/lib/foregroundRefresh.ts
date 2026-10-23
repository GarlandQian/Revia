/** 原生焦点监听异步安装；切换目标或卸载时，迟到的安装结果也必须释放。 */
export function subscribeToForeground(
  listen: (onFocus: (focused: boolean) => void) => Promise<() => void>,
  onForeground: () => void,
  onError: (caught: unknown) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  void listen((focused) => {
    if (!disposed && focused) onForeground();
  }).then((stop) => {
    if (disposed) stop();
    else unlisten = stop;
  }).catch((caught) => {
    if (!disposed) onError(caught);
  });
  return () => {
    if (disposed) return;
    disposed = true;
    unlisten?.();
  };
}
