#!/bin/bash

set -euo pipefail

# declare -a HOSTS=("cloud", "civitas")
declare -a HOSTS=("cloud.spacetimedb.com")

usage() {
	echo "Usage: upload.sh <ssh-key-file-path>"
}

if [ "$#" != 1 ] ; then
	usage
	exit 1
fi

if [ ! -f "$1" ] ; then
	usage
	echo "File not found: $1"
	exit
fi

echo "This script will upload to all of the environments listed below:"
echo
for host in "${HOSTS[@]}" ; do
	printf "\t%s\n" "${host}"
done

echo
echo "If you want to add a host, use Ctrl-C to exit now and add the host to this script."
echo "Otherwise, press [Enter] now to continue."
read -rp ""

# Build the sdk
bash ./export.sh

for host in "${HOSTS[@]}" ; do
	scp -oStrictHostKeyChecking=no -i "$1" "SpacetimeDBUnitySDK.unitypackage" "root@${host}:/var/www/sdk/SpacetimeDBUnitySDK.unitypackage"
	ssh -oStrictHostKeyChecking=no -i "$1" "root@${host}" "chown -R jenkins:jenkins /var/www/sdk"
done
