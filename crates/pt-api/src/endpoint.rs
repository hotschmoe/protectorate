use crate::schema::{Described, TypeDef};

pub const API_PREFIX: &str = "/api/";
pub const EVENTS_PATH: &str = "/api/events";
pub const FROM_SEQ_PARAM: &str = "from_seq";
pub const ACCESS_TOKEN_PARAM: &str = "access_token";

pt_json::string_enum! {
    /// Who may call a method.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Scope {
        /// The owner token (CLI) and `pd1.` device tokens (apps).
        Owner = "owner",
        /// Also a sleeve's `pc1.` capability, for its own sleeve and its children. Mail also
        /// reaches its parent and Envoy. Envoy's capability reaches every sleeve, since Envoy
        /// drives them.
        Sleeve = "sleeve",
        /// Also the envoy-only capability of vendor general agents (D9).
        EnvoyOnly = "envoy_only",
        /// No credential: the request carries its own proof (a one-time pairing code).
        Public = "public",
    }
}

impl Scope {
    /// Whether a credential holding `self` may call a method of scope `method`.
    pub fn allows(self, method: Scope) -> bool {
        match self {
            Scope::Owner => true,
            Scope::Sleeve => method != Scope::Owner,
            Scope::EnvoyOnly => matches!(method, Scope::EnvoyOnly | Scope::Public),
            Scope::Public => method == Scope::Public,
        }
    }
}

/// One API method, `POST /api/<NAME>`.
pub trait Method {
    const NAME: &'static str;
    const SCOPE: Scope;
    type Request: Described;
    type Response: Described;
}

/// A method as data, for walking the table.
pub struct MethodDef {
    pub name: &'static str,
    pub doc: &'static str,
    pub scope: Scope,
    pub request: fn() -> &'static TypeDef,
    pub response: fn() -> &'static TypeDef,
}

impl MethodDef {
    pub fn path(&self) -> String {
        format!("{API_PREFIX}{}", self.name)
    }
}

/// Called once per method, in table order.
pub trait MethodVisitor {
    fn visit<M: Method>(&mut self);
}

/// The method table: a `method::<Name>` type per method, `METHODS` and `visit_methods`.
macro_rules! api_methods {
    ($( $(#[doc = $doc:literal])* $name:ident($req:ty) -> $resp:ty : $scope:ident ),* $(,)?) => {
        /// One zero-sized type per method, implementing [`Method`](crate::Method).
        pub mod method {
            #[allow(unused_imports)]
            use super::*;
            $(
                $(#[doc = $doc])*
                pub struct $name;

                impl $crate::Method for $name {
                    const NAME: &'static str = ::core::stringify!($name);
                    const SCOPE: $crate::Scope = $crate::Scope::$scope;
                    type Request = $req;
                    type Response = $resp;
                }
            )*
        }

        /// Every method, in table order.
        pub static METHODS: &[$crate::MethodDef] = &[$(
            $crate::MethodDef {
                name: ::core::stringify!($name),
                doc: ::core::concat!($($doc, "\n",)*),
                scope: $crate::Scope::$scope,
                request: <$req as $crate::Described>::def,
                response: <$resp as $crate::Described>::def,
            },
        )*];

        /// Calls `v.visit::<M>()` for every method, in table order.
        pub fn visit_methods(v: &mut impl $crate::MethodVisitor) {
            $( v.visit::<method::$name>(); )*
        }
    };
}
