# IAM EKS user mapper
This tool aims to automatically give selected AWS IAM users access to your Kubernetes cluster. 
It's based on this [tool](https://github.com/ygrene/iam-eks-user-mapper) which is now archived, its main features were reported and extended (role based auth for example).

## Design overview
![](doc/images/design-overview-dark.svg)

IAM EKS user mapper is running as a pod in the kubernetes cluster.
At a given interval (default 30s) it executes the following:
1. gets IAM users from IAM groups to be given access to the cluster
2. add IAM users from IAM groups `aws-auth` configmap in the cluster giving them access to the cluster

## Usage
```shell
./iam-eks-user-mapper \
    --service-account-name <SERVICE_ACCOUNT_NAME> \
    --aws-role-arn <AWS_ROLE_ARN> \
    --aws-default-region <AWS_DEFAULT_REGION> \
    --iam-k8s-groups <IAM_K8S_GROUPS> \
    --refresh-interval-seconds <REFRESH_INTERVAL_SECONDS> \
    --verbose <VERBOSE>
```

| Parameter                  | Type      | Default | Required | Description                                                                                                            | Example                                 |
|----------------------------|-----------|---------|----------|------------------------------------------------------------------------------------------------------------------------|-----------------------------------------|
| `service-account-name`     | `String`  |         | `true`   | Service account name to be used                                                                                        | `my-service-account`                    |
| `aws-role-arn`             | `String`  |         | `true`   | AWS role ARN to be used                                                                                                | `arn:aws:iam::12345678910:role/my-role` |
| `aws_default_region`       | `String`  |         | `true`   | AWS default region to be used                                                                                          | `eu-west-3`                             |
| `refresh_interval_seconds` | `Integer` | `30`    | `false`  | Refresh interval in seconds between two user synchronization                                                           | `120`                                   |
| `iam_k8s_groups`           | `String`  |         | `true`   | IAM groups to be mapped into Kubernetes, syntax is `<IAM_GROUP>-><KUBERNETES_GROUP>,<IAM_GROUP_2>-><KUBERNETES_GROUP_2>` | `Admins->system:masters`, `Admins->system:masters,Devops->system:devops` |
| `verbose`                  | `Boolean` | `false` | `false`  | Activate verbose mode | `Admins->system:masters`, `Admins->system:masters,Devops->system:devops` |

All parameters can be set as environment variables as well:

```shell
SERVICE_ACCOUNT_NAME=<SERVICE_ACCOUNT_NAME> \
AWS_ROLE_ARN=<AWS_ROLE_ARN> \
AWS_DEFAULT_REGION=<AWS_DEFAULT_REGION> \
IAM_K8S_GROUPS=<IAM_K8S_GROUPS> \
REFRESH_INTERVAL_SECONDS=<REFRESH_INTERVAL_SECONDS> \
VERBOSE=<VERBOSE> \
./iam-eks-user-mapper
```

### Helm
Giving a `iam-eks-user-mapper.yaml` file with the following content:
```yaml
iamK8sGroups: <IAM_K8S_GROUPS>
refreshIntervalSeconds: <REFRESH_INTERVAL_SECONDS>

aws:
  defaultRegion: <AWS_DEFAULT_REGION>
  roleArn: <AWS_ROLE_ARN>

# Repository for the image is there
# https://github.com/Qovery/iam-eks-user-mapper
image:
  repository: docker pull ghcr.io/qovery/iam-eks-user-mapper
  pullPolicy: IfNotPresent
  tag: main

serviceAccount:
  name: <AWS_ROLE_ARN>
  annotations:
    - eks\\.amazonaws\\.com/role-arn=<SERVICE_ACCOUNT_NAME>

resources:
  limits:
    cpu: <RESOURCES_LIMITS_CPU>
    memory: <RESOURCES_LIMITS_MEMORY>
  requests:
    cpu: <RESOURCES_REQUESTS_CPU>
    memory: <RESOURCES_REQUESTS_MEMORY>
```

```shell
helm upgrade \
    --kubeconfig <YOUR_KUBECONFIG_FILE_PATH> \
    --install --namespace "kube-system" \
    -f "iam-eks-user-mapper.yaml" \
    iam-eks-user-mapper ./charts/iam-eks-user-mapper"
```


### Cargo
``` shell
git clone https://github.com/Qovery/iam-eks-user-mapper.git && cd $_

cargo run -- \
    --service-account-name <SERVICE_ACCOUNT_NAME> \
    --aws-role-arn <AWS_ROLE_ARN> \
    --aws-default-region <AWS_DEFAULT_REGION> \
    --iam-k8s-groups <IAM_K8S_GROUPS> \
    --refresh-interval-seconds <REFRESH_INTERVAL_SECONDS> \
    --verbose <VERBOSE>
```

### Docker
```shell
docker run ghcr.io/qovery/iam-eks-user-mapper:main \
    -e IAM_K8S_GROUPS="<IAM_K8S_GROUPS>" \
    -e REFRESH_INTERVAL_SECONDS="<REFRESH_INTERVAL_SECONDS>" \
    -e IAM_K8S_GROUPS="<IAM_K8S_GROUPS>" \
    -e AWS_DEFAULT_REGION="<AWS_DEFAULT_REGION>" \
    -e AWS_ROLE_ARN="<AWS_ROLE_ARN>" \
    -e SERVICE_ACCOUNT_NAME="<SERVICE_ACCOUNT_NAME>" \
```

## Want to contribute?
This tool is far from perfect and we will be happy to have people helping making it better.
You can either:
- open an issue for bugs / enhancements
- open a PR linked to an issue
- pick an issue and submit a PR
