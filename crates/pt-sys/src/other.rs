use std::convert::Infallible;
use std::io;
use std::process::Command;
use std::time::Duration;

/// Never constructed on this OS: `install_term_handler` returns `Unsupported`.
#[derive(Clone)]
pub struct TermFlag(Infallible);

impl TermFlag {
    pub fn is_set(&self) -> bool {
        match self.0 {}
    }
    pub fn signal(&self) -> Option<i32> {
        match self.0 {}
    }
    pub fn wait(&self) {
        match self.0 {}
    }
    pub fn wait_timeout(&self, _timeout: Duration) -> bool {
        match self.0 {}
    }
}

pub fn install_term_handler() -> io::Result<TermFlag> {
    Err(crate::unsupported("termination signal handling"))
}

pub fn set_pdeathsig(_sig: i32) -> io::Result<()> {
    Err(crate::unsupported("PR_SET_PDEATHSIG"))
}

pub fn pdeathsig_on_spawn(_cmd: &mut Command, _sig: i32) -> io::Result<()> {
    Err(crate::unsupported("PR_SET_PDEATHSIG"))
}
