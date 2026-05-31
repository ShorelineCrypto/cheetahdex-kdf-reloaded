use lazy_static::lazy_static;
use mm2_metamask::{Eip712, FieldKind, TypeDef};

const LOGIN_TYPE_NAME: &str = "AtomicDEXLogin";

lazy_static! {
    static ref LOGIN_TYPES: [TypeDef; 2] = build_login_types();
}

/// Assembles the full EIP-712 request for an AtomicDEX login signature.
pub(crate) fn build_login_eip712(domain: LoginDomain, message: LoginMessage) -> Eip712<LoginDomain, LoginMessage> {
    let types = LOGIN_TYPES
        .iter()
        .map(|def| (def.name.clone(), def.fields.clone()))
        .collect();
    Eip712 {
        types,
        domain,
        primary_type: LOGIN_TYPE_NAME.to_string(),
        message,
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct LoginDomain {
    name: String,
}

impl LoginDomain {
    pub fn new(name: String) -> Self { LoginDomain { name } }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct LoginMessage {
    message: String,
}

impl LoginMessage {
    pub fn new(project_name: String) -> Self {
        LoginMessage {
            message: format!("Login to {project_name}"),
        }
    }
}

fn build_login_types() -> [TypeDef; 2] {
    let mut domain = TypeDef::domain();
    domain.field("name", FieldKind::String);

    let mut login = TypeDef::new(LOGIN_TYPE_NAME);
    login.field("message", FieldKind::String);

    [domain, login]
}
