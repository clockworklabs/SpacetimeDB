#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test performs all of the steps of the BitCraftMini getting started guide that I could automate quickly. Basically after this you just have to start up BitCraftMini and see if its working properly"
        exit
fi

set -euox pipefail

source "./test/lib.include"

[ -d ../BitCraftMini ]

# 2. Compile the Spacetime Module
run_test cargo run publish -S --project-path "../BitCraftMini/Server" --clear-database
ADDRESS="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"
sleep 2
mkdir -p ../BitCraftMini/Client/Assets/_Project/autogen
run_test cargo run generate --out-dir ../BitCraftMini/Client/Assets/_Project/autogen --lang=cs --project-path "../BitCraftMini/Server"
