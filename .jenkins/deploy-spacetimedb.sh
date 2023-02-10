#!/bin/bash

# This script deploys the SpacetimeDB live environment.

usage() {
	echo "Usage:"
	echo "\t$0 <do-registry-username> <do-registery-password> <partner-name>"
}

cd /home/jenkins
set -euo pipefail

if [ $# != 3 ] ; then
	echo "ERROR: Invalid number of params!"
	usage
	exit;
fi

docker login -u ${1} -p ${2} https://registry.digitalocean.com
cd SpacetimeDB 
echo "REGISTRY_SUFFIX=-partner-${3}" > .env
docker-compose -f docker-compose-live.yml pull
docker-compose -f docker-compose-live.yml up -d
