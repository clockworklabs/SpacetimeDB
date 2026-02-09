/// The trait used as bound on `v8::ArrayBuffer::new_backing_store_from_bytes`
/// isn't public, so we need to emulate it.
pub(super) trait IntoArrayBufferBackingStore {
    fn into_backing_store(self) -> v8::UniqueRef<v8::BackingStore>;
}
macro_rules! impl_into_backing_store {
    ([$($bounds:tt)*] $t:ty) => {
        impl<$($bounds)*> IntoArrayBufferBackingStore for $t {
            fn into_backing_store(self) -> v8::UniqueRef<v8::BackingStore> {
                v8::ArrayBuffer::new_backing_store_from_bytes(self)
            }
        }
    };
    ($($primitive:ty),*) => {$(
        impl_into_backing_store!([] Box<[$primitive]>);
        impl_into_backing_store!([] Vec<$primitive>);
    )*};
}

impl_into_backing_store!([T: AsMut<[u8]>] Box<T>);
impl_into_backing_store!(u8, u16, u32, u64, i8, i16, i32, i64);

/// Taking a scope and a buffer, return a `v8::Local<'scope, v8::Uint8Array>`.
pub(super) fn make_uint8array<'scope>(
    scope: &v8::PinScope<'scope, '_>,
    buf: impl IntoArrayBufferBackingStore,
) -> v8::Local<'scope, v8::Uint8Array> {
    let store = buf.into_backing_store();
    let len = store.byte_length();
    let buf = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
    v8::Uint8Array::new(scope, buf, 0, len).expect("len > 8 pebibytes")
}

/// Taking a scope and a buffer, return a `v8::Local<'scope, v8::DataView>`.
pub(super) fn make_dataview<'scope>(
    scope: &v8::PinScope<'scope, '_>,
    buf: impl IntoArrayBufferBackingStore,
) -> v8::Local<'scope, v8::DataView> {
    let store = buf.into_backing_store();
    let len = store.byte_length();
    let buf = v8::ArrayBuffer::with_backing_store(scope, &store.make_shared());
    v8::DataView::new(scope, buf, 0, len)
}
