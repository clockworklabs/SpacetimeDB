#!/bin/bash

set -euo pipefail

ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-spacetimedb-2 sudo docker restart spacetimedb &
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-spacetimedb-3 sudo docker restart spacetimedb &
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-spacetimedb-4 sudo docker restart spacetimedb &
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-spacetimedb-5 sudo docker restart spacetimedb &
# ssh debug@bitcraft-test-spacetimedb-6 docker restart spacetimedb &
# ssh debug@bitcraft-test-spacetimedb-7 docker restart spacetimedb &
ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-spacetimedb-controller sudo docker restart spacetimedb &

wait

ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null debug@bitcraft-test-tools-1 sudo docker restart bitcraft-relay-server
