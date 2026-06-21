#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Activate,
    Deactivate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pulse {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    ButtonA(Edge),
    ButtonB(Edge),
    ButtonC(Edge),
    ButtonD(Edge),
    ButtonE(Edge),
    ButtonF(Edge),
    ExpressionPedalA(u16),
    ExpressionPedalB(u16),
    VolButton(Edge),
    Vol(Pulse),
    GainButton(Edge),
    Gain(Pulse),
}
