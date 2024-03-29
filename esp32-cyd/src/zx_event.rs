use rustzx_core::zx::keys::ZXKey;

pub enum Event {
    NoEvent,
    ZXKey(ZXKey, bool),
    ZXKeyWithModifier(ZXKey, ZXKey, bool),
}