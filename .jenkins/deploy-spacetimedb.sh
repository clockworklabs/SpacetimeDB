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
if [ ! -f .env ] ; then
	echo "WARNING: Environment file not found. This file should have already been configured. Adding required fields now."
	echo "REGISTRY_SUFFIX=-partner-${3}" > .env
fi
docker-compose -f docker-compose-live.yml pull
docker-compose -f docker-compose-live.yml up -d
