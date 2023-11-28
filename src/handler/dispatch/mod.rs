mod live_effect;
mod live_looper;
mod setup_looper;
mod test;

use crate::handler::{Actions, Handler, HandlerVec};
use crate::hmi::inputs::InputEvent;
use crate::hmi::leds::Leds;
use core::fmt;
use heapless::Vec;

/// Enum based static dispatch
pub enum HandlerEnum {
    LiveEffect(self::live_effect::LiveEffect),
    LiveLooper(self::live_looper::LiveLooper),
    SetupLooper(self::setup_looper::SetupLooper),
    Test(self::test::Test),
}

impl Handler for HandlerEnum {
    fn handle_human_input(&mut self, e: InputEvent) -> Actions {
        match self {
            HandlerEnum::LiveEffect(h) => h.handle_human_input(e),
            HandlerEnum::LiveLooper(h) => h.handle_human_input(e),
            HandlerEnum::SetupLooper(h) => h.handle_human_input(e),
            HandlerEnum::Test(h) => h.handle_human_input(e),
        }
    }
    fn leds(&mut self) -> &mut Leds {
        match self {
            HandlerEnum::LiveEffect(h) => h.leds(),
            HandlerEnum::LiveLooper(h) => h.leds(),
            HandlerEnum::SetupLooper(h) => h.leds(),
            HandlerEnum::Test(h) => h.leds(),
        }
    }
}

impl fmt::Debug for HandlerEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HandlerEnum::LiveEffect(_) => f.debug_tuple("LiveEffect").finish(),
            HandlerEnum::LiveLooper(_) => f.debug_tuple("LiveLooper").finish(),
            HandlerEnum::SetupLooper(_) => f.debug_tuple("SetupLooper").finish(),
            HandlerEnum::Test(_) => f.debug_tuple("Test").finish(),
        }
    }
}

pub fn create() -> HandlerVec<HandlerEnum> {
    let mut handlers: crate::handler::HandlerVec<crate::handler::dispatch::HandlerEnum> =
        Vec::new();
    handlers
        .push(crate::handler::dispatch::HandlerEnum::LiveEffect(
            self::live_effect::LiveEffect::new(),
        ))
        .unwrap();
    handlers
        .push(crate::handler::dispatch::HandlerEnum::LiveLooper(
            self::live_looper::LiveLooper::new(),
        ))
        .unwrap();
    handlers
        .push(crate::handler::dispatch::HandlerEnum::SetupLooper(
            self::setup_looper::SetupLooper::new(),
        ))
        .unwrap();
    handlers
        .push(crate::handler::dispatch::HandlerEnum::Test(
            self::test::Test::new(),
        ))
        .unwrap();
    handlers
}
