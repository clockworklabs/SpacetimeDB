#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test tests to make sure that the default namespace is working properly"
        exit
fi

set -euox pipefail

source "./test/lib.include"

TMP_DIR=$(mktemp -d)
NAMESPACE=SpacetimeDB

reset_project
run_test spacetime generate --out-dir "${TMP_DIR}" --lang cs --project-path "${PROJECT_PATH}"

LINES="$(grep -r -o "namespace ${NAMESPACE}" "${TMP_DIR}" | wc -l | tr -d ' ')"
if [ "${LINES}" != 4 ] ; then
	echo "FOUND: ${LINES} EXPECTED: "
	exit 1
fi

echo "${TMP_DIR}"
#rm -rf "${TMP_DIR}"
