/// One release asset and the name it gets inside `releases/<version>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub asset: String,
    pub install_as: String,
    pub executable: bool,
}
