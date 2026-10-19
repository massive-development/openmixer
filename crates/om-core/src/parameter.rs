use crate::ParamId;

/// Legacy string control surface: ids are `"prefix.rest"` strings, routed by longest prefix.
/// Engine objects implement [`ParamTarget`] and reach this only through the router's parse step.
pub trait ParameterTarget: Send + Sync {
    /// Returns `false` if the id is unknown. Safe to call from any non-audio thread. The router runs
    /// this while holding its gate, so do not block here: a slow target stalls all dispatch.
    fn set_parameter(&self, id: &str, value: f64) -> bool;
}

/// Typed control surface for one engine namespace. The id is already parsed; the target matches the
/// variants it owns and returns `false` for the rest. Safe to call from any non-audio thread.
pub trait ParamTarget: Send + Sync {
    fn set_param(&self, id: &ParamId, value: f64) -> bool;
}
