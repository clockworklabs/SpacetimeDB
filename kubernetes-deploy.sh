#! /bin/bash
# exit script when any command ran here returns with non-zero exit code
set -e

if [ "$BUILD_ENV" == "live" ]; then
  export HOST_URL="bitcraftonline.com"
fi

if [ "$BUILD_ENV" == "staging" ]; then
  export HOST_URL="staging.bitcraftonline.com"
fi

if [ "$BUILD_ENV" == "qa" ]; then
  export HOST_URL="qa.bitcraftonline.com"
fi

if [ "$BUILD_ENV" == "testing" ]; then
  export HOST_URL="testing.bitcraftonline.com"
fi

# substitute env variables
for FILE in $(find ./kube -name "*.yaml" -print)
do
  envsubst < "$FILE" > "$FILE".out
  mv "$FILE".out "$FILE"
done

echo "$KUBERNETES_CLUSTER_CERTIFICATE" | base64 --decode > cert.crt

FLAGS=( "--kubeconfig=/dev/null" "--server=$KUBERNETES_SERVER" "--certificate-authority=cert.crt" "--token=$KUBERNETES_TOKEN" )

# deploy nginx ingress service
kubectl "${FLAGS[@]}" apply -f https://raw.githubusercontent.com/kubernetes/ingress-nginx/controller-v1.1.1/deploy/static/provider/aws/deploy.yaml

# deploy cert-manager
kubectl "${FLAGS[@]}" apply -f https://github.com/jetstack/cert-manager/releases/download/v1.3.0/cert-manager.yaml

# deploy issuer resource and load balancer resource
kubectl "${FLAGS[@]}" create namespace "$BUILD_ENV" || true # ignore error if namespace already exists
kubectl "${FLAGS[@]}" create -f ./kube/issuer.yaml || true # ignore error if issuer already exists
kubectl "${FLAGS[@]}" apply -f ./kube/cloud-generic.yaml

SELECTED_FILES=(./kube/*.yaml)
SELECTED_FILES+=(./kube/$BUILD_ENV/*.yaml)

for FILE in "${SELECTED_FILES[@]}"
do
  if test -f "$FILE"; then
    echo "kubectl ${FLAGS[*]} apply -f $FILE"
    kubectl "${FLAGS[@]}" apply -f "$FILE"
  fi
d
