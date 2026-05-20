use crate::core::auth::models::{Principal, PrincipalRole};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Capability {
    GscaleCatalogRead,
    GscalePrint,
    RpsBatchManage,
}

pub fn has_capability(principal: &Principal, capability: Capability) -> bool {
    match capability {
        Capability::GscaleCatalogRead | Capability::GscalePrint | Capability::RpsBatchManage => {
            matches!(principal.role, PrincipalRole::Admin | PrincipalRole::Werka)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gscale_and_rps_capabilities_are_admin_or_werka_only() {
        let admin = principal(PrincipalRole::Admin);
        let werka = principal(PrincipalRole::Werka);
        let supplier = principal(PrincipalRole::Supplier);
        let customer = principal(PrincipalRole::Customer);

        for capability in [
            Capability::GscaleCatalogRead,
            Capability::GscalePrint,
            Capability::RpsBatchManage,
        ] {
            assert!(has_capability(&admin, capability));
            assert!(has_capability(&werka, capability));
            assert!(!has_capability(&supplier, capability));
            assert!(!has_capability(&customer, capability));
        }
    }

    fn principal(role: PrincipalRole) -> Principal {
        Principal {
            role,
            display_name: String::new(),
            legal_name: String::new(),
            ref_: String::new(),
            phone: String::new(),
            avatar_url: String::new(),
        }
    }
}
