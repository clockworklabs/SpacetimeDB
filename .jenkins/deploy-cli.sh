#!/bin/bash

# This deploy script is uploaded to the server that hosts the spacetimedb
# website. This deploy script is in charge of moving the freshly-built cli tools
# to where they need to go.

set -euo pipefail

set -x

deploy() {
	if [ $# != 2 ] ; then
		echo "Deploy requires 2 parameters, input file and destination."
		exit 1;
	fi

	if [ ! -f "cli-bin/$1" ] ; then
		echo "Input file $1 does not exist."
		exit 1
	fi

	mkdir -p "$(dirname "$2")"
	mv -v "cli-bin/$1" "$2"
}

if [ ! -d "cli-bin" ] ; then
	echo "cli-bin directory is missing."
	exit 1
fi

echo "Deploying files..."
deploy spacetime.linux /var/www/install/spacetime.linux
deploy spacetime.macos /var/www/install/spacetime.macos
deploy spacetime.exe /var/www/winows/spacetime.exe
