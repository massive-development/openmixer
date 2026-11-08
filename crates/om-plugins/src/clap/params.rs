//! CLAP parameters as host data.

/// One parameter a plugin declares.
#[derive(Debug, Clone, PartialEq)]
pub struct ClapParam {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub stepped: bool,
    pub hidden: bool,
    pub read_only: bool,
}
