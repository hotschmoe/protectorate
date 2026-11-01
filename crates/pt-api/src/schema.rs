//! The schema description every API type registers, and the macros that declare API types.
//!
//! Each macro expands to the matching `pt_json` macro (so the JSON shape is pt-json's) and to an
//! `impl Described` whose static [`TypeDef`] lists the fields, defaults, enum values and union
//! variants. `pt-apigen` walks these tables to write the Dart client; the fuzz target walks them
//! to build values.

use std::time::{SystemTime, UNIX_EPOCH};

use pt_json::{Error, FromJson, ToJson, Value};

/// A field's wire type.
#[derive(Clone, Copy)]
pub enum Ty {
    Bool,
    I64,
    U64,
    F64,
    Str,
    /// Unix epoch milliseconds ([`EpochMs`]); the field name ends in `_ms`.
    Time,
    /// Any JSON value, passed through untouched.
    Json,
    /// Absent or null on the wire.
    Opt(fn() -> Ty),
    List(fn() -> Ty),
    /// An object with string keys.
    Map(fn() -> Ty),
    Named(fn() -> &'static TypeDef),
}

pub struct TypeDef {
    pub name: &'static str,
    pub doc: &'static str,
    pub shape: Shape,
}

#[derive(Clone, Copy)]
pub enum Shape {
    Record(&'static [FieldDef]),
    Enum(&'static [EnumValue]),
    /// Internally tagged: `{"type": "<tag>", ...fields}`.
    Union(&'static [VariantDef]),
}

pub struct FieldDef {
    /// The JSON key.
    pub name: &'static str,
    pub doc: &'static str,
    pub ty: fn() -> Ty,
    /// The value used when the key is missing or null, as JSON.
    pub default: Option<fn() -> Value>,
}

pub struct EnumValue {
    pub wire: &'static str,
    pub doc: &'static str,
}

pub struct VariantDef {
    pub tag: &'static str,
    /// The Rust variant name.
    pub name: &'static str,
    pub doc: &'static str,
    pub fields: &'static [FieldDef],
}

/// An API type with a schema description.
pub trait Described: ToJson + FromJson + Clone + PartialEq + std::fmt::Debug {
    fn def() -> &'static TypeDef;
}

/// A type that may appear as a field.
pub trait ApiType {
    fn ty() -> Ty;
}

macro_rules! prim {
    ($($t:ty => $v:ident),* $(,)?) => {$(
        impl ApiType for $t {
            fn ty() -> Ty {
                Ty::$v
            }
        }
    )*};
}

prim!(bool => Bool, i64 => I64, u64 => U64, f64 => F64, String => Str, Value => Json);

impl<T: ApiType> ApiType for Option<T> {
    fn ty() -> Ty {
        Ty::Opt(T::ty)
    }
}

impl<T: ApiType> ApiType for Vec<T> {
    fn ty() -> Ty {
        Ty::List(T::ty)
    }
}

impl<T: ApiType> ApiType for std::collections::BTreeMap<String, T> {
    fn ty() -> Ty {
        Ty::Map(T::ty)
    }
}

/// A point in time as Unix epoch milliseconds, a JSON integer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EpochMs(pub u64);

impl EpochMs {
    pub fn now() -> EpochMs {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        EpochMs(u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

impl ToJson for EpochMs {
    fn to_json(&self) -> Value {
        self.0.to_json()
    }
}

impl FromJson for EpochMs {
    fn from_json(v: &Value) -> Result<Self, Error> {
        u64::from_json(v).map(EpochMs)
    }
}

impl ApiType for EpochMs {
    fn ty() -> Ty {
        Ty::Time
    }
}

macro_rules! api_field {
    ($field:ident, $fty:ty, $doc:expr) => {
        $crate::schema::FieldDef {
            name: ::core::stringify!($field),
            doc: $doc,
            ty: <$fty as $crate::schema::ApiType>::ty,
            default: ::core::option::Option::None,
        }
    };
    ($field:ident, $fty:ty, $doc:expr, $default:expr) => {
        $crate::schema::FieldDef {
            name: ::core::stringify!($field),
            doc: $doc,
            ty: <$fty as $crate::schema::ApiType>::ty,
            default: ::core::option::Option::Some({
                fn default() -> ::pt_json::Value {
                    let v: $fty = $default;
                    ::pt_json::ToJson::to_json(&v)
                }
                default as fn() -> ::pt_json::Value
            }),
        }
    };
}

macro_rules! api_type {
    ($name:ident, $doc:expr, $shape:expr) => {
        impl $crate::schema::Described for $name {
            fn def() -> &'static $crate::schema::TypeDef {
                static DEF: $crate::schema::TypeDef = $crate::schema::TypeDef {
                    name: ::core::stringify!($name),
                    doc: $doc,
                    shape: $shape,
                };
                &DEF
            }
        }

        impl $crate::schema::ApiType for $name {
            fn ty() -> $crate::schema::Ty {
                $crate::schema::Ty::Named(<$name as $crate::schema::Described>::def)
            }
        }
    };
}

/// Structs, as `pt_json::record!` (all fields `pub`, docs as `///` only).
macro_rules! api_records {
    ($(
        $(#[doc = $doc:literal])*
        pub struct $name:ident {
            $( $(#[doc = $fdoc:literal])* pub $field:ident : $fty:ty $(= $default:expr)? ),* $(,)?
        }
    )*) => {$(
        ::pt_json::record! {
            $(#[doc = $doc])*
            #[derive(Debug, Clone, PartialEq)]
            pub struct $name {
                $( $(#[doc = $fdoc])* pub $field: $fty $(= $default)?, )*
            }
        }

        api_type!($name, ::core::concat!($($doc, "\n",)*), $crate::schema::Shape::Record(&[
            $( api_field!($field, $fty, ::core::concat!($($fdoc, "\n",)*) $(, $default)?), )*
        ]));
    )*};
}

/// String enums, as `pt_json::string_enum!`.
macro_rules! api_enums {
    ($(
        $(#[doc = $doc:literal])*
        pub enum $name:ident {
            $( $(#[doc = $vdoc:literal])* $variant:ident = $text:literal ),* $(,)?
        }
    )*) => {$(
        ::pt_json::string_enum! {
            $(#[doc = $doc])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum $name {
                $( $(#[doc = $vdoc])* $variant = $text, )*
            }
        }

        api_type!($name, ::core::concat!($($doc, "\n",)*), $crate::schema::Shape::Enum(&[
            $( $crate::schema::EnumValue {
                wire: $text,
                doc: ::core::concat!($($vdoc, "\n",)*),
            }, )*
        ]));
    )*};
}

/// Internally tagged unions, as `pt_json::tagged_enum!`.
macro_rules! api_unions {
    ($(
        $(#[doc = $doc:literal])*
        pub enum $name:ident {
            $( $(#[doc = $vdoc:literal])* $variant:ident = $tag:literal {
                $( $(#[doc = $fdoc:literal])* $field:ident : $fty:ty $(= $default:expr)? ),* $(,)?
            } ),* $(,)?
        }
    )*) => {$(
        ::pt_json::tagged_enum! {
            $(#[doc = $doc])*
            #[derive(Debug, Clone, PartialEq)]
            pub enum $name {
                $( $(#[doc = $vdoc])* $variant = $tag {
                    $( $(#[doc = $fdoc])* $field: $fty $(= $default)?, )*
                }, )*
            }
        }

        api_type!($name, ::core::concat!($($doc, "\n",)*), $crate::schema::Shape::Union(&[
            $( $crate::schema::VariantDef {
                tag: $tag,
                name: ::core::stringify!($variant),
                doc: ::core::concat!($($vdoc, "\n",)*),
                fields: &[
                    $( api_field!($field, $fty, ::core::concat!($($fdoc, "\n",)*) $(, $default)?), )*
                ],
            }, )*
        ]));
    )*};
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
