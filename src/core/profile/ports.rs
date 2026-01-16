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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadedFile {
    pub content_type: String,
    pub body: Vec<u8>,
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

    async fn download_file(&self, file_url: &str) -> Result<DownloadedFile, ProfilePortError>;
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ProfilePortError {
    #[error("lookup failed")]
    LookupFailed,
}
