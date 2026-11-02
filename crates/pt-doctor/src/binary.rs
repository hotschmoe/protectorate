use pt_json::{record, string_enum};

string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Install {
        Native = "native",
        Npm = "npm",
        Script = "script",
        Other = "other",
        Missing = "missing",
    }
}

record! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Binary {
        pub name: String,
        pub present: bool,
        pub path: String,
        /// `path` with symlinks resolved, and through a shell wrapper's `exec` of an absolute path;
        /// the install method is read from this file.
        pub real_path: String,
        pub install: Install,
        pub version: String,
        pub error: String,
    }
}
