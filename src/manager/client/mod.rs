use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::{Request, Status};

use crate::pb::service::budget::budget_service_client::BudgetServiceClient;
use crate::pb::service::budget::{BudgetRole, CheckRoleRequest};

pub struct BudgetClient {
    inner: BudgetServiceClient<Channel>,
}

impl BudgetClient {
    pub async fn connect(url: &str) -> Result<Self, tonic::transport::Error> {
        let channel = Channel::from_shared(url.to_string())
            .expect("invalid budget gRPC URL")
            .connect()
            .await?;
        Ok(Self {
            inner: BudgetServiceClient::new(channel),
        })
    }

    pub async fn check_role(
        &mut self,
        user_id: &str,
        budget_id: &str,
        user_type: Option<&str>,
    ) -> Result<BudgetRole, Status> {
        tracing::debug!(
            "BudgetClient.check_role: user_id={}, budget_id={}, user_type={:?}",
            user_id,
            budget_id,
            user_type
        );
        let mut req = Request::new(CheckRoleRequest {
            user_id: user_id.to_string(),
            budget_id: budget_id.to_string(),
        });
        if let Ok(v) = MetadataValue::try_from(user_id) {
            req.metadata_mut().insert("x-user-id", v);
        }
        if let Some(ut) = user_type {
            tracing::debug!("BudgetClient.check_role: inserting x-user-type={}", ut);
            if let Ok(v) = MetadataValue::try_from(ut) {
                req.metadata_mut().insert("x-user-type", v);
            }
        }
        let resp = self.inner.check_role(req).await?;
        Ok(BudgetRole::try_from(resp.into_inner().role).unwrap_or(BudgetRole::Unspecified))
    }
}
