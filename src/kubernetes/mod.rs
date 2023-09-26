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
    #[error("Error while trying to serialize roles map to YAML: {raw_message}")]
    CannotSerializeRolesMap { raw_message: Arc<str> },
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

#[derive(Eq, PartialEq)]
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

#[derive(Eq, PartialEq, Clone)]
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

#[derive(Eq, PartialEq)]
pub struct KubernetesUser {
    pub iam_user_name: IamUserName,
    pub iam_arn: IamArn,
    pub roles: HashSet<KubernetesGroupName>,
}

impl KubernetesUser {
    pub fn new(
        iam_user_name: IamUserName,
        iam_arn: IamArn,
        roles: HashSet<KubernetesGroupName>,
    ) -> KubernetesUser {
        KubernetesUser {
            iam_user_name,
            iam_arn,
            roles,
        }
    }
}

impl Hash for KubernetesUser {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iam_user_name.to_string().to_lowercase().hash(state);
        self.iam_arn.to_string().to_lowercase().hash(state);
    }
}

#[derive(Eq, PartialEq, Clone)]
pub struct KubernetesRole {
    pub iam_role_arn: IamArn,
    pub role_name: Option<String>,
    pub user_name: Option<String>,
    pub groups: HashSet<KubernetesGroupName>,
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

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MapUserConfig {
    #[serde(rename = "userarn")]
    user_arn: String,
    #[serde(rename = "username")]
    username: String,
    #[serde(rename = "groups")]
    groups: HashSet<String>,
}

impl From<KubernetesUser> for MapUserConfig {
    fn from(value: KubernetesUser) -> Self {
        MapUserConfig {
            user_arn: value.iam_arn.to_string(),
            username: value.iam_user_name.to_string(),
            groups: value.roles.iter().map(|r| r.to_string()).collect(),
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
    rolename: Option<String>,
    #[serde(rename = "username")]
    username: Option<String>,
    #[serde(rename = "groups")]
    groups: HashSet<String>,
}

impl From<KubernetesRole> for MapRoleConfig {
    fn from(value: KubernetesRole) -> Self {
        MapRoleConfig {
            role_arn: value.iam_role_arn.to_string(),
            rolename: value.role_name,
            username: value.user_name,
            groups: value.groups.iter().map(|g| g.to_string()).collect(),
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
        kubernetes_users: Option<HashSet<KubernetesUser>>,
        kubernetes_sso_role: Option<KubernetesRole>,
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

        // adding users
        if let Some(kubernetes_users_to_add) = kubernetes_users {
            config_map_data.insert(
                "mapUsers".to_string(),
                Self::generate_users_config_map_yaml_string(kubernetes_users_to_add)?,
            );
        }

        // adding sso role if not there already
        if let Some(sso_role_to_add) = kubernetes_sso_role {
            let mut kubernetes_roles: HashSet<KubernetesRole> = config_map_data
                .get("mapRoles")
                .map(|raw| {
                    let raw_roles: HashSet<MapRoleConfig> =
                        serde_yaml::from_str(raw).unwrap_or_default();

                    raw_roles
                        .iter()
                        .map(|r| KubernetesRole {
                            role_name: r.rolename.clone(),
                            user_name: r.username.clone(),
                            iam_role_arn: IamArn(r.role_arn.to_string()),
                            groups: r
                                .groups
                                .iter()
                                .map(|g| KubernetesGroupName(g.to_string()))
                                .collect(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            if !kubernetes_roles.contains(&sso_role_to_add) {
                // role is not there, adding it
                kubernetes_roles.insert(sso_role_to_add);

                config_map_data.insert(
                    "mapRoles".to_string(),
                    Self::generate_roles_config_map_yaml_string(kubernetes_roles)?,
                );
            }
        }

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
        IamArn, IamUserName, KubernetesError, KubernetesGroupName, KubernetesService,
        KubernetesUser, MapUserConfig,
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
                    },
                    KubernetesUser {
                        iam_user_name: IamUserName::new("user_2"),
                        iam_arn: IamArn::new("arn:test:user_2"),
                        roles: HashSet::from_iter(vec![
                            KubernetesGroupName::new("group_2"),
                            KubernetesGroupName::new("group_3"),
                        ]),
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
    - group_3"
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
                input: HashSet::from_iter(vec![]),
                expected_output: Ok(r"".to_string()),
                _description: "case 3 - no users",
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
}
