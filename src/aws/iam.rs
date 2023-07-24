use crate::aws::{AwsError, AwsSdkConfig};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IamError {
    #[error("Cannot get users from group `{group}`, error: {raw_message}")]
    CannotGetUserFromGroup {
        group: Arc<str>,
        raw_message: Arc<str>,
    },
}

pub struct IamService {
    client: aws_sdk_iam::Client,
}

type Arn = String;
type Group = String;

#[derive(Eq, PartialEq)]
pub struct AwsUser {
    pub arn: Arn,
    pub user_name: String,
    pub groups: Vec<Group>,
}

impl IamService {
    pub fn new(config: &AwsSdkConfig) -> Self {
        IamService {
            client: aws_sdk_iam::Client::new(&config.config),
        }
    }

    pub async fn get_users_from_groups(
        &self,
        groups_names: Vec<&str>,
    ) -> Result<HashSet<AwsUser>, AwsError> {
        let mut all_users = HashSet::new();

        for group_name in groups_names {
            match self.get_users_from_group(group_name).await {
                Ok(users) => all_users.extend(users),
                Err(e) => return Err(e),
            }
        }

        Ok(all_users)
    }

    pub async fn get_users_from_group(
        &self,
        group_name: &str,
    ) -> Result<HashSet<AwsUser>, AwsError> {
        let mut users: HashSet<AwsUser> = HashSet::new();

        match self.client.get_group().group_name(group_name).send().await {
            Ok(group) => {
                for user in group.users().unwrap_or_default() {
                    let mut aws_user = AwsUser {
                        arn: user.arn().unwrap_or_default().to_string(),
                        user_name: user.user_name().unwrap_or_default().to_string(),
                        groups: vec![group_name.to_string()],
                    };

                    if let Some(u) = users.get(&aws_user) {
                        aws_user.groups.extend(u.groups.clone());
                    }

                    users.insert(aws_user);
                }
            }
            Err(e) => {
                return Err(AwsError::IamError {
                    underlying_error: IamError::CannotGetUserFromGroup {
                        group: Arc::from(group_name),
                        raw_message: Arc::from(e.to_string()),
                    },
                })
            }
        }

        Ok(users)
    }
}

impl Hash for AwsUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.user_name.to_lowercase().hash(state);
        self.arn.to_lowercase().hash(state);
    }
}
