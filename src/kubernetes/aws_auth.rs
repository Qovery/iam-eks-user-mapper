use crate::kubernetes::{KubernetesRole, KubernetesUser, SyncedBy};
use std::collections::HashSet;

pub struct AwsAuth {
    pub users: HashSet<KubernetesUser>,
    pub roles: HashSet<KubernetesRole>,
}

pub struct AwsAuthBuilder {
    users: HashSet<KubernetesUser>,
    roles: HashSet<KubernetesRole>,

    new_synced_users: HashSet<KubernetesUser>,
    new_synced_roles: HashSet<KubernetesRole>,
}

impl AwsAuthBuilder {
    pub fn new(users: HashSet<KubernetesUser>, roles: HashSet<KubernetesRole>) -> AwsAuthBuilder {
        AwsAuthBuilder {
            users: users
                .into_iter()
                .filter(|u| match u.synced_by {
                    // removing all users managed by the tool (allowing to delete previously synced users)
                    Some(SyncedBy::IamEksUserMapper) => false,
                    _ => true,
                })
                .collect(),
            roles: roles
                .into_iter()
                .filter(|r| match r.synced_by {
                    // removing all roles managed by the tool (allowing to delete previously synced users)
                    Some(SyncedBy::IamEksUserMapper) => false,
                    _ => true,
                })
                .collect(),

            new_synced_users: HashSet::default(),
            new_synced_roles: HashSet::default(),
        }
    }

    pub fn new_synced_users(&mut self, u: HashSet<KubernetesUser>) -> &mut Self {
        self.new_synced_users = u
            .into_iter()
            .map(|u| KubernetesUser::new_synced_from(u, SyncedBy::IamEksUserMapper))
            .collect(); // make sure those users are set to synced

        self
    }

    pub fn new_synced_roles(&mut self, r: HashSet<KubernetesRole>) -> &mut Self {
        self.new_synced_roles = r
            .into_iter()
            .map(|r| KubernetesRole::new_synced_from(r, SyncedBy::IamEksUserMapper))
            .collect();

        self
    }

    pub fn build(&self) -> AwsAuth {
        // computing users
        let mut kubernetes_users: HashSet<KubernetesUser> = HashSet::from_iter(
            self.users
                .clone()
                .into_iter()
                // remove users already there but not flagged as synced since those will be added
                .filter(|u| !self.new_synced_users.contains(u)),
        );
        // adding new synced users
        kubernetes_users.extend(self.new_synced_users.clone());

        // computing roles
        let mut kubernetes_roles: HashSet<KubernetesRole> = HashSet::from_iter(
            self.roles
                .clone()
                .into_iter()
                // remove roles already there but not flagged as synced since those will be added
                .filter(|r| !self.new_synced_roles.contains(r)),
        );
        // adding new synced roles
        kubernetes_roles.extend(self.new_synced_roles.clone());

        AwsAuth {
            users: kubernetes_users,
            roles: kubernetes_roles,
        }
    }
}

impl From<AwsAuth> for AwsAuthBuilder {
    fn from(value: AwsAuth) -> Self {
        AwsAuthBuilder {
            users: value.users,
            roles: value.roles,

            new_synced_users: HashSet::default(),
            new_synced_roles: HashSet::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::kubernetes::aws_auth::AwsAuthBuilder;
    use crate::kubernetes::{
        IamArn, IamUserName, KubernetesGroupName, KubernetesRole, KubernetesUser, SyncedBy,
    };
    use std::collections::HashSet;

    #[test]
    fn aws_auth_build_users_test() {
        // setup:
        struct TestCase<'a> {
            existing_users: HashSet<KubernetesUser>,
            new_users_to_be_added: HashSet<KubernetesUser>,
            expected_users: HashSet<KubernetesUser>,
            _description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                existing_users: HashSet::default(),
                new_users_to_be_added: HashSet::default(),
                expected_users: HashSet::default(),
                _description: "case 1: no existing users, no new users",
            },
            TestCase {
                existing_users: HashSet::default(),
                new_users_to_be_added: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                _description: "case 2: no existing users, some new users",
            },
            TestCase {
                existing_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_users_to_be_added: HashSet::default(),
                expected_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                _description: "case 3: existing users, no new users",
            },
            TestCase {
                existing_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_users_to_be_added: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_2"),
                    IamArn::new("arn:::::user_2"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_users: HashSet::from_iter(vec![
                    KubernetesUser::new(
                        IamUserName::new("user_1"),
                        IamArn::new("arn:::::user_1"),
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_1"),
                            KubernetesGroupName::new("group_2"),
                        ]),
                        None,
                    ),
                    KubernetesUser::new(
                        IamUserName::new("user_2"),
                        IamArn::new("arn:::::user_2"),
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_2"),
                            KubernetesGroupName::new("group_3"),
                        ]),
                        Some(SyncedBy::IamEksUserMapper),
                    ),
                ]),
                _description: "case 4: existing users, some new users",
            },
            TestCase {
                existing_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_users_to_be_added: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_users: HashSet::from_iter(vec![KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                _description: "case 5: existing user without synced by flag, same new user with new synced by field",
            },
        ];

        for tc in test_cases {
            // execute:
            let result = AwsAuthBuilder::new(tc.existing_users, HashSet::default())
                .new_synced_users(tc.new_users_to_be_added)
                .build();

            // verify:
            assert_eq!(tc.expected_users, result.users);
        }
    }

    #[test]
    fn aws_auth_build_add_new_synced_users_should_add_synced_field_test() {
        // setup:
        let test_cases: Vec<HashSet<KubernetesUser>> = vec![
            HashSet::default(),
            HashSet::from_iter(vec![
                KubernetesUser::new(
                    IamUserName::new("user_1"),
                    IamArn::new("arn:::::user_1"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                ),
                KubernetesUser::new(
                    IamUserName::new("user_2"),
                    IamArn::new("arn:::::user_2"),
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    None,
                ),
            ]),
            HashSet::from_iter(vec![KubernetesUser::new(
                IamUserName::new("user_2"),
                IamArn::new("arn:::::user_2"),
                HashSet::from_iter(vec![
                    KubernetesGroupName::new("group_2"),
                    KubernetesGroupName::new("group_3"),
                ]),
                Some(SyncedBy::Unknown),
            )]),
            HashSet::from_iter(vec![KubernetesUser::new(
                IamUserName::new("user_2"),
                IamArn::new("arn:::::user_2"),
                HashSet::from_iter(vec![
                    KubernetesGroupName::new("group_2"),
                    KubernetesGroupName::new("group_3"),
                ]),
                Some(SyncedBy::IamEksUserMapper),
            )]),
        ];

        for tc in test_cases {
            // execute:
            let result = AwsAuthBuilder::new(HashSet::default(), HashSet::default())
                .new_synced_users(tc.clone())
                .build();

            // verify:
            assert_eq!(tc.len(), result.users.iter().len());
            assert!(result
                .users
                .iter()
                .all(|u| u.synced_by == Some(SyncedBy::IamEksUserMapper)));
        }
    }

    #[test]
    fn aws_auth_build_roles_test() {
        // setup:
        struct TestCase<'a> {
            existing_roles: HashSet<KubernetesRole>,
            new_roles_to_be_added: HashSet<KubernetesRole>,
            expected_roles: HashSet<KubernetesRole>,
            _description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                existing_roles: HashSet::default(),
                new_roles_to_be_added: HashSet::default(),
                expected_roles: HashSet::default(),
                _description: "case 1: no existing roles, no new roles",
            },
            TestCase {
                existing_roles: HashSet::default(),
                new_roles_to_be_added: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_roles: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                _description: "case 2: no existing roles, some new roles",
            },
            TestCase {
                existing_roles: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_roles_to_be_added: HashSet::default(),
                expected_roles: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                _description: "case 3: existing roles, no new roles",
            },
            TestCase {
                existing_roles: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_roles_to_be_added: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_2"),
                    Some("role_2".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_roles: HashSet::from_iter(vec![
                    KubernetesRole::new(
                        IamArn::new("arn:::::role_1"),
                        Some("role_1".to_string()),
                        None,
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_1"),
                            KubernetesGroupName::new("group_2"),
                        ]),
                        None,
                    ),
                    KubernetesRole::new(
                        IamArn::new("arn:::::role_2"),
                        Some("role_2".to_string()),
                        None,
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_2"),
                            KubernetesGroupName::new("group_3"),
                        ]),
                        Some(SyncedBy::IamEksUserMapper),
                    ),
                ]),
                _description: "case 4: existing roles, some new roles",
            },
            TestCase {
                existing_roles: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                )]),
                new_roles_to_be_added: HashSet::from_iter(vec![KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    Some(SyncedBy::IamEksUserMapper),
                )]),
                expected_roles: HashSet::from_iter(vec![
                    KubernetesRole::new(
                        IamArn::new("arn:::::role_1"),
                        Some("role_1".to_string()),
                        None,
                        HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_1"),
                            KubernetesGroupName::new("group_2"),
                        ]),
                        Some(SyncedBy::IamEksUserMapper),
                    ),
                ]),
                _description: "case 5: existing role without synced by flag, same new role with new synced by field",
            },
        ];

        for tc in test_cases {
            // execute:
            let result = AwsAuthBuilder::new(HashSet::default(), tc.existing_roles)
                .new_synced_roles(tc.new_roles_to_be_added)
                .build();

            // verify:
            assert_eq!(tc.expected_roles, result.roles);
        }
    }

    #[test]
    fn aws_auth_build_add_new_synced_roles_should_add_synced_field_test() {
        // setup:
        let test_cases: Vec<HashSet<KubernetesRole>> = vec![
            HashSet::default(),
            HashSet::from_iter(vec![
                KubernetesRole::new(
                    IamArn::new("arn:::::role_1"),
                    Some("role_1".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    None,
                ),
                KubernetesRole::new(
                    IamArn::new("arn:::::role_2"),
                    Some("role_2".to_string()),
                    None,
                    HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    None,
                ),
            ]),
            HashSet::from_iter(vec![KubernetesRole::new(
                IamArn::new("arn:::::role_2"),
                Some("role_2".to_string()),
                None,
                HashSet::from_iter(vec![
                    KubernetesGroupName::new("group_2"),
                    KubernetesGroupName::new("group_3"),
                ]),
                Some(SyncedBy::Unknown),
            )]),
            HashSet::from_iter(vec![KubernetesRole::new(
                IamArn::new("arn:::::role_2"),
                Some("role_2".to_string()),
                None,
                HashSet::from_iter(vec![
                    KubernetesGroupName::new("group_2"),
                    KubernetesGroupName::new("group_3"),
                ]),
                Some(SyncedBy::IamEksUserMapper),
            )]),
        ];

        for tc in test_cases {
            // execute:
            let result = AwsAuthBuilder::new(HashSet::default(), HashSet::default())
                .new_synced_roles(tc.clone())
                .build();

            // verify:
            assert_eq!(tc.len(), result.roles.iter().len());
            assert!(result
                .roles
                .iter()
                .all(|u| u.synced_by == Some(SyncedBy::IamEksUserMapper)));
        }
    }
}
