mod aws_auth;

use crate::kubernetes::aws_auth::AwsAuthBuilder;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::PostParams;
use kube::{Api, Client};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum KubernetesError {
    #[error("Cluster not reachable: {raw_message}")]
    ClusterUnreachable { raw_message: Arc<str> },
    #[error("Error while trying to serialize users map to YAML: {raw_message}")]
    CannotSerializeUsersMap { raw_message: Arc<str> },
    #[error("Error while trying to deserialize users map from YAML: {raw_message}")]
    CannotDeserializeUsersMap {
        raw_message: Arc<str>,
        underlying_error: Arc<str>,
    },
    #[error("Error while trying to serialize roles map to YAML: {raw_message}")]
    CannotSerializeRolesMap { raw_message: Arc<str> },
    #[error("Error while trying to deserialize roles map from YAML: {raw_message}")]
    CannotDeserializeRolesMap {
        raw_message: Arc<str>,
        underlying_error: Arc<str>,
    },
    #[error("Cannot find config map `{config_map_name}` in namespace `{config_map_namespace}`: {raw_message}")]
    ConfigMapNotFound {
        config_map_name: Arc<str>,
        config_map_namespace: Arc<str>,
        raw_message: Arc<str>,
    },
    #[error("Cannot patch config map `{config_map_name}` in namespace `{config_map_namespace}`: {raw_message}")]
    ConfigMapCannotBePatched {
        config_map_name: Arc<str>,
        config_map_namespace: Arc<str>,
        raw_message: Arc<str>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SyncedBy {
    #[serde(rename = "iam-eks-user-mapper")]
    IamEksUserMapper,
    #[serde(rename = "unknown")]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IamUserName(String);

impl IamUserName {
    pub fn new(iam_user_name: &str) -> IamUserName {
        IamUserName(iam_user_name.to_string())
    }
}

impl Display for IamUserName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Eq, PartialEq)]
pub struct IamRoleName(String);

impl Display for IamRoleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct IamArn(String);

impl IamArn {
    pub fn new(iam_arn: &str) -> IamArn {
        IamArn(iam_arn.to_string())
    }
}

impl Display for IamArn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct KubernetesGroupName(String);

impl KubernetesGroupName {
    pub fn new(kubernetes_role: &str) -> KubernetesGroupName {
        KubernetesGroupName(kubernetes_role.to_string())
    }
}

impl Display for KubernetesGroupName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct KubernetesUser {
    pub iam_user_name: IamUserName,
    pub iam_arn: IamArn,
    pub roles: HashSet<KubernetesGroupName>,
    pub synced_by: Option<SyncedBy>,
}

impl KubernetesUser {
    pub fn new(
        iam_user_name: IamUserName,
        iam_arn: IamArn,
        roles: HashSet<KubernetesGroupName>,
        synced_by: Option<SyncedBy>,
    ) -> KubernetesUser {
        KubernetesUser {
            iam_user_name,
            iam_arn,
            roles,
            synced_by,
        }
    }

    pub fn new_synced_from(u: KubernetesUser, synced_by: SyncedBy) -> KubernetesUser {
        let mut synced_u = u.clone();
        synced_u.synced_by = Some(synced_by);

        synced_u
    }
}

impl From<MapUserConfig> for KubernetesUser {
    fn from(value: MapUserConfig) -> Self {
        KubernetesUser {
            iam_user_name: IamUserName(value.username),
            iam_arn: IamArn(value.user_arn),
            roles: HashSet::from_iter(value.groups.into_iter().map(KubernetesGroupName)),
            synced_by: value.synced_by,
        }
    }
}

impl Hash for KubernetesUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iam_user_name.to_string().to_lowercase().hash(state);
        self.iam_arn.to_string().to_lowercase().hash(state);
    }
}

impl PartialEq for KubernetesUser {
    fn eq(&self, other: &Self) -> bool {
        self.roles == other.roles
            && self.iam_arn == other.iam_arn
            && self.iam_user_name == other.iam_user_name
    }
}

impl Eq for KubernetesUser {}

#[derive(Debug, Clone)]
pub struct KubernetesRole {
    pub iam_role_arn: IamArn,
    pub role_name: Option<String>,
    pub user_name: Option<String>,
    pub groups: HashSet<KubernetesGroupName>,
    pub synced_by: Option<SyncedBy>,
}

impl KubernetesRole {
    pub fn new(
        iam_role_arn: IamArn,
        role_name: Option<String>,
        user_name: Option<String>,
        groups: HashSet<KubernetesGroupName>,
        synced_by: Option<SyncedBy>,
    ) -> Self {
        Self {
            iam_role_arn,
            role_name,
            user_name,
            groups,
            synced_by,
        }
    }
    pub fn new_synced_from(r: KubernetesRole, synced_by: SyncedBy) -> KubernetesRole {
        let mut synced_r = r.clone();
        synced_r.synced_by = Some(synced_by);

        synced_r
    }
}

impl Hash for KubernetesRole {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iam_role_arn.to_string().to_lowercase().hash(state);
        if let Some(role_name) = &self.role_name {
            role_name.to_string().to_lowercase().hash(state);
        }
        if let Some(user_name) = &self.user_name {
            user_name.to_string().to_lowercase().hash(state);
        }
    }
}

impl PartialEq for KubernetesRole {
    fn eq(&self, other: &Self) -> bool {
        self.groups == other.groups
            && self.iam_role_arn == other.iam_role_arn
            && self.user_name == other.user_name
            && self.role_name == other.role_name
    }
}

impl Eq for KubernetesRole {}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MapUserConfig {
    #[serde(rename = "userarn")]
    user_arn: String,
    #[serde(rename = "username")]
    username: String,
    #[serde(rename = "groups")]
    groups: HashSet<String>,
    #[serde(rename = "syncedBy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    synced_by: Option<SyncedBy>,
}

impl From<KubernetesUser> for MapUserConfig {
    fn from(value: KubernetesUser) -> Self {
        MapUserConfig {
            user_arn: value.iam_arn.to_string(),
            username: value.iam_user_name.to_string(),
            groups: value.roles.iter().map(|r| r.to_string()).collect(),
            synced_by: value.synced_by,
        }
    }
}

impl Hash for MapUserConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.user_arn.to_lowercase().hash(state);
        self.username.to_lowercase().hash(state);
    }
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MapRoleConfig {
    #[serde(rename = "rolearn")]
    role_arn: String,
    #[serde(rename = "rolename")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    rolename: Option<String>,
    #[serde(rename = "username")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    username: Option<String>,
    #[serde(rename = "groups")]
    groups: HashSet<String>,
    #[serde(rename = "syncedBy")]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    synced_by: Option<SyncedBy>,
}

impl From<KubernetesRole> for MapRoleConfig {
    fn from(value: KubernetesRole) -> Self {
        MapRoleConfig {
            role_arn: value.iam_role_arn.to_string(),
            rolename: value.role_name,
            username: value.user_name,
            groups: value.groups.iter().map(|g| g.to_string()).collect(),
            synced_by: value.synced_by,
        }
    }
}

impl Hash for MapRoleConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.role_arn.to_lowercase().hash(state);
        if let Some(rolename) = &self.rolename {
            rolename.to_lowercase().hash(state);
        }
        if let Some(username) = &self.username {
            username.to_lowercase().hash(state);
        }
    }
}

pub struct KubernetesService {
    client: Client,
}

impl KubernetesService {
    pub async fn new() -> Result<KubernetesService, KubernetesError> {
        let kube_client =
            Client::try_default()
                .await
                .map_err(|e| KubernetesError::ClusterUnreachable {
                    raw_message: Arc::from(e.to_string()),
                })?;

        Ok(KubernetesService {
            client: kube_client,
        })
    }

    fn generate_users_config_map_yaml_string(
        kubernetes_users: HashSet<KubernetesUser>,
    ) -> Result<String, KubernetesError> {
        let user_config_map: HashSet<MapUserConfig> =
            HashSet::from_iter(kubernetes_users.into_iter().map(MapUserConfig::from));

        match serde_yaml::to_string(&user_config_map) {
            Ok(s) => Ok(s),
            Err(e) => Err(KubernetesError::CannotSerializeUsersMap {
                raw_message: Arc::from(e.to_string()),
            }),
        }
    }

    fn generate_roles_config_map_yaml_string(
        kubernetes_roles: HashSet<KubernetesRole>,
    ) -> Result<String, KubernetesError> {
        let role_config_map: HashSet<MapRoleConfig> =
            HashSet::from_iter(kubernetes_roles.into_iter().map(MapRoleConfig::from));

        match serde_yaml::to_string(&role_config_map) {
            Ok(s) => Ok(s),
            Err(e) => Err(KubernetesError::CannotSerializeRolesMap {
                raw_message: Arc::from(e.to_string()),
            }),
        }
    }

    pub async fn update_user_and_role_config_map(
        &self,
        config_map_namespace: &str,
        config_map_name: &str,
        kubernetes_users_to_be_added: Option<HashSet<KubernetesUser>>,
        kubernetes_sso_role_to_be_added: Option<KubernetesRole>,
        karpenter_role_to_be_added: Option<KubernetesRole>,
    ) -> Result<(), KubernetesError> {
        let config_maps_api: Api<ConfigMap> =
            Api::namespaced(self.client.clone(), config_map_namespace); // TODO(benjaminch): avoid clone()

        // get config map
        let mut users_config_map = config_maps_api.get(config_map_name).await.map_err(|e| {
            KubernetesError::ConfigMapNotFound {
                config_map_name: Arc::from(config_map_name),
                config_map_namespace: Arc::from(config_map_namespace),
                raw_message: Arc::from(e.to_string()),
            }
        })?;

        // update config map
        let mut default_config_map_data = BTreeMap::new();
        let config_map_data = users_config_map
            .data
            .as_mut()
            .unwrap_or(&mut default_config_map_data);

        let aws_auth = AwsAuthBuilder::new(
            // get existing users from configmap
            match config_map_data.get("mapUsers") {
                None => HashSet::with_capacity(0),
                Some(kubernetes_existing_users_raw_yaml) => HashSet::from_iter(
                    serde_yaml::from_str::<HashSet<MapUserConfig>>(
                        kubernetes_existing_users_raw_yaml,
                    )
                    .map_err(|e| KubernetesError::CannotDeserializeUsersMap {
                        raw_message: Arc::from(kubernetes_existing_users_raw_yaml.as_str()),
                        underlying_error: Arc::from(e.to_string().as_str()),
                    })?
                    .into_iter()
                    .map(KubernetesUser::from)
                    .collect::<Vec<_>>(),
                ),
            },
            // get existing roles from configmap
            match config_map_data.get("mapRoles") {
                None => HashSet::with_capacity(0),
                Some(kubernetes_existing_roles_raw_yaml) => HashSet::from_iter(
                    serde_yaml::from_str::<HashSet<MapRoleConfig>>(
                        kubernetes_existing_roles_raw_yaml,
                    )
                    .map_err(|e| KubernetesError::CannotDeserializeRolesMap {
                        raw_message: Arc::from(kubernetes_existing_roles_raw_yaml.as_str()),
                        underlying_error: Arc::from(e.to_string().as_str()),
                    })?
                    .into_iter()
                    .map(|r| KubernetesRole {
                        role_name: r.rolename.clone(),
                        user_name: r.username.clone(),
                        iam_role_arn: IamArn(r.role_arn.to_string()),
                        groups: r
                            .groups
                            .iter()
                            .map(|g| KubernetesGroupName(g.to_string()))
                            .collect(),
                        synced_by: r.synced_by.clone(),
                    })
                    .collect::<Vec<_>>(),
                ),
            },
        )
        .new_synced_users(kubernetes_users_to_be_added.unwrap_or_default())
        .new_synced_roles({
            let mut roles = Vec::new();
            if let Some(sso_role) = kubernetes_sso_role_to_be_added {
                roles.append(&mut vec![sso_role])
            };
            if let Some(karpenter_role) = karpenter_role_to_be_added {
                roles.append(&mut vec![karpenter_role])
            };
            HashSet::from_iter(roles)
        })
        .build();

        // adding users
        config_map_data.insert(
            "mapUsers".to_string(),
            Self::generate_users_config_map_yaml_string(aws_auth.users)?,
        );

        // adding sso roles
        config_map_data.insert(
            "mapRoles".to_string(),
            Self::generate_roles_config_map_yaml_string(aws_auth.roles)?,
        );

        match config_maps_api
            .replace(config_map_name, &PostParams::default(), &users_config_map)
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(KubernetesError::ConfigMapCannotBePatched {
                config_map_name: Arc::from(config_map_name),
                config_map_namespace: Arc::from(config_map_namespace),
                raw_message: Arc::from(e.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::kubernetes::{
        IamArn, IamUserName, KubernetesError, KubernetesGroupName, KubernetesRole,
        KubernetesService, KubernetesUser, MapRoleConfig, MapUserConfig, SyncedBy,
    };
    use std::collections::HashSet;

    #[test]
    fn generate_users_config_map_yaml_string_test() {
        // setup:
        struct TestCase<'a> {
            input: HashSet<KubernetesUser>,
            expected_output: Result<String, KubernetesError>,
            _description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                input: HashSet::from_iter(vec![
                    KubernetesUser {
                        iam_user_name: IamUserName::new("user_1"),
                        iam_arn: IamArn::new("arn:test:user_1"),
                        roles: HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_1"),
                            KubernetesGroupName::new("group_2"),
                        ]),
                        synced_by: None,
                    },
                    KubernetesUser {
                        iam_user_name: IamUserName::new("user_2"),
                        iam_arn: IamArn::new("arn:test:user_2"),
                        roles: HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_2"),
                            KubernetesGroupName::new("group_3"),
                        ]),
                        synced_by: None,
                    },
                    KubernetesUser {
                        iam_user_name: IamUserName::new("user_3"),
                        iam_arn: IamArn::new("arn:test:user_3"),
                        roles: HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_3"),
                            KubernetesGroupName::new("group_4"),
                        ]),
                        synced_by: Some(SyncedBy::IamEksUserMapper),
                    },
                ]),
                expected_output: Ok(r"
- userarn: arn:test:user_1
  username: user_1
  groups:
    - group_1
    - group_2
- userarn: arn:test:user_2
  username: user_2
  groups:
    - group_2
    - group_3
- userarn: arn:test:user_3
  username: user_3
  groups:
    - group_3
    - group_4
  syncedBy: iam-eks-user-mapper"
                    .trim_start()
                    .to_string()),

                _description: "case 1 - nominal case",
            },
            TestCase {
                input: HashSet::from_iter(vec![KubernetesUser {
                    iam_user_name: IamUserName::new("user_1"),
                    iam_arn: IamArn::new("arn:test:user_1"),
                    roles: HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    synced_by: None,
                }]),
                expected_output: Ok(r"
- userarn: arn:test:user_1
  username: user_1
  groups:
    - group_1
    - group_2"
                    .trim_start()
                    .to_string()),

                _description: "case 2 - one user",
            },
            TestCase {
                input: HashSet::from_iter(vec![KubernetesUser {
                    iam_user_name: IamUserName::new("user_1"),
                    iam_arn: IamArn::new("arn:test:user_1"),
                    roles: HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_1"),
                        KubernetesGroupName::new("group_2"),
                    ]),
                    synced_by: Some(SyncedBy::Unknown),
                }]),
                expected_output: Ok(r"
- userarn: arn:test:user_1
  username: user_1
  groups:
    - group_1
    - group_2
  syncedBy: a-tool-we-do-not-know"
                    .trim_start()
                    .to_string()),

                _description: "case 3 - one user synced by unknown provider",
            },
            TestCase {
                input: HashSet::from_iter(vec![]),
                expected_output: Ok(r"".to_string()),
                _description: "case 4 - no users",
            },
        ];

        for tc in test_cases {
            // execute:
            let result = KubernetesService::generate_users_config_map_yaml_string(tc.input);

            // verify:
            match tc.expected_output {
                Ok(expected_yaml) => {
                    assert!(result.is_ok());

                    // YAML serializer is not preserving orders
                    let parsed_yaml_expected_result: Result<HashSet<MapUserConfig>, _> =
                        serde_yaml::from_str(&expected_yaml);
                    assert!(parsed_yaml_expected_result.is_ok());

                    let result_yaml_string = result.unwrap_or_default();
                    let parsed_yaml_result: Result<HashSet<MapUserConfig>, _> =
                        serde_yaml::from_str(&result_yaml_string);
                    assert!(parsed_yaml_result.is_ok());

                    assert_eq!(
                        parsed_yaml_expected_result.unwrap_or_default(),
                        parsed_yaml_result.unwrap_or_default()
                    );
                }
                Err(e) => {
                    assert!(result.is_err());
                    assert_eq!(e, result.unwrap_err());
                }
            }
        }
    }

    #[test]
    fn generate_roles_config_map_yaml_string_test() {
        // setup:
        struct TestCase<'a> {
            input: HashSet<KubernetesRole>,
            expected_output: Result<String, KubernetesError>,
            _description: &'a str,
        }

        let test_cases = vec![
            TestCase {
                input: HashSet::from_iter(vec![KubernetesRole {
                    role_name: Some("role_1".to_string()),
                    user_name: None,
                    iam_role_arn: IamArn::new("arn:test:role_1"),
                    groups: HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    synced_by: None,
                }]),
                expected_output: Ok(r"
- rolearn: arn:test:role_1
  rolename: role_1
  groups:
    - group_2
    - group_3"
                    .trim_start()
                    .to_string()),

                _description: "case 1 - nominal case",
            },
            TestCase {
                input: HashSet::from_iter(vec![KubernetesRole {
                    role_name: Some("role_1".to_string()),
                    user_name: None,
                    iam_role_arn: IamArn::new("arn:test:role_1"),
                    groups: HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    synced_by: Some(SyncedBy::IamEksUserMapper),
                }]),
                expected_output: Ok(r"
- rolearn: arn:test:role_1
  rolename: role_1
  groups:
    - group_2
    - group_3
  syncedBy: iam-eks-user-mapper"
                    .trim_start()
                    .to_string()),

                _description: "case 2 - role synced by iam-user-mapper",
            },
            TestCase {
                input: HashSet::from_iter(vec![KubernetesRole {
                    role_name: Some("role_1".to_string()),
                    user_name: None,
                    iam_role_arn: IamArn::new("arn:test:role_1"),
                    groups: HashSet::from_iter(vec![
                        KubernetesGroupName::new("group_2"),
                        KubernetesGroupName::new("group_3"),
                    ]),
                    synced_by: Some(SyncedBy::Unknown),
                }]),
                expected_output: Ok(r"
- rolearn: arn:test:role_1
  rolename: role_1
  groups:
    - group_2
    - group_3
  syncedBy: some-tool-we-do-not-know"
                    .trim_start()
                    .to_string()),

                _description: "case 3 - role synced by unknown",
            },
        ];

        for tc in test_cases {
            // execute:
            let result = KubernetesService::generate_roles_config_map_yaml_string(tc.input);

            // verify:
            match tc.expected_output {
                Ok(expected_yaml) => {
                    assert!(result.is_ok());

                    // YAML serializer is not preserving orders
                    let parsed_yaml_expected_result: Result<HashSet<MapRoleConfig>, _> =
                        serde_yaml::from_str(&expected_yaml);
                    assert!(parsed_yaml_expected_result.is_ok());

                    let result_yaml_string = result.unwrap_or_default();
                    let parsed_yaml_result: Result<HashSet<MapRoleConfig>, _> =
                        serde_yaml::from_str(&result_yaml_string);
                    assert!(parsed_yaml_result.is_ok());

                    assert_eq!(
                        parsed_yaml_expected_result.unwrap_or_default(),
                        parsed_yaml_result.unwrap_or_default()
                    );
                }
                Err(e) => {
                    assert!(result.is_err());
                    assert_eq!(e, result.unwrap_err());
                }
            }
        }
    }
}
