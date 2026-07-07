/// 把一个普通 `Stream` 包一层，在"非正常 drop"（未读到 EOF）时触发 hook。
/// 配合 `StreamedFile.on_abort` 让 service 层能在客户端断连时做清理。
pub(super) struct AbortAwareStream<S> {
    pub(super) inner: S,
    pub(super) hook: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl<S: futures::Stream + Unpin> futures::Stream for AbortAwareStream<S> {
    type Item = S::Item;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let res = std::pin::Pin::new(&mut self.inner).poll_next(cx);
        if let std::task::Poll::Ready(None) = &res {
            // 走到 EOF，解除 abort hook
            self.hook = None;
        }
        res
    }
}

impl<S> Drop for AbortAwareStream<S> {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            hook();
        }
    }
}
