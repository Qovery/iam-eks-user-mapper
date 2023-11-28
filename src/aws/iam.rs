use crate::aws::AwsSdkConfig;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IamError {
    #[error("Cannot get users from IAM group `{group}`, error: {raw_message}")]
    CannotGetUserFromIamGroup {
        group: IamGroup,
        raw_message: Arc<str>,
    },
    #[error("No users found in IAM group `{group}`")]
    NoUsersFoundInIamGroup { group: IamGroup },
}

#[derive(Eq, PartialEq)]
pub struct Arn(String);

impl Arn {
    pub fn new(arn: &str) -> Arn {
        Arn(arn.to_string())
    }
}

impl Display for Arn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IamGroup(String);

impl IamGroup {
    pub fn new(iam_group_name: &str) -> IamGroup {
        IamGroup(iam_group_name.to_string())
    }
}

impl Display for IamGroup {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Eq, PartialEq)]
pub struct User(String);

impl User {
    pub fn new(user: &str) -> User {
        User(user.to_string())
    }
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Eq, PartialEq)]
pub struct AwsUser {
    pub arn: Arn,
    pub user_name: User,
    pub groups: HashSet<IamGroup>,
}

impl Hash for AwsUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.user_name.to_string().to_lowercase().hash(state);
        self.arn.to_string().to_lowercase().hash(state);
    }
}

pub struct IamService {
    client: aws_sdk_iam::Client,
    _verbose: bool,
}

impl IamService {
    pub fn new(config: &AwsSdkConfig, verbose: bool) -> Self {
        IamService {
            client: aws_sdk_iam::Client::new(&config.config),
            _verbose: verbose,
        }
    }

    pub async fn get_users_from_groups(
        &self,
        iam_groups: HashSet<IamGroup>,
    ) -> Result<HashSet<AwsUser>, IamError> {
        let mut all_users = HashSet::new();

        for iam_group in iam_groups {
            match self.get_users_from_group(&iam_group).await {
                Ok(users) => all_users.extend(users),
                Err(e) => return Err(e),
            }
        }

        Ok(all_users)
    }

    pub async fn get_users_from_group(
        &self,
        iam_group: &IamGroup,
    ) -> Result<HashSet<AwsUser>, IamError> {
        let mut users: HashSet<AwsUser> = HashSet::new();

        match self
            .client
            .get_group()
            .group_name(iam_group.to_string())
            .send()
            .await
        {
            Ok(group) => {
                let group_users = group.users();

                if group_users.is_empty() {
                    return Err(IamError::NoUsersFoundInIamGroup {
                        group: iam_group.clone(),
                    });
                }

                for user in group_users {
                    let mut aws_user = AwsUser {
                        arn: Arn::new(user.arn()),
                        user_name: User::new(user.user_name()),
                        groups: HashSet::from_iter(vec![iam_group.clone()]),
                    };

                    if let Some(u) = users.get(&aws_user) {
                        aws_user.groups.extend(u.groups.clone());
                    }

                    users.insert(aws_user);
                }
            }
            Err(e) => {
                return Err(IamError::CannotGetUserFromIamGroup {
                    group: iam_group.clone(),
                    raw_message: Arc::from(e.to_string()),
                })
            }
        }

        Ok(users)
    }
}
