use async_trait::async_trait;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplierProfileRecord {
    pub phone: String,
    pub image: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomerProfileRecord {
    pub phone: String,
}

#[async_trait]
pub trait ProfileLookup: Send + Sync {
    async fn get_supplier_profile(
        &self,
        id: &str,
    ) -> Result<SupplierProfileRecord, ProfilePortError>;

    async fn get_customer_profile(
        &self,
        id: &str,
    ) -> Result<CustomerProfileRecord, ProfilePortError>;
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ProfilePortError {
    #[error("lookup failed")]
    LookupFailed,
}
