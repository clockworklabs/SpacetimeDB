#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This test tests to sure that when a custom namespace is specified on the command line, it actually gets used in generation"
        exit
fi

set -euox pipefail

source "./test/lib.include"

TMP_DIR=$(mktemp -d)
NAMESPACE=$(random_string)

reset_project
run_test "$SPACETIME" generate --out-dir "${TMP_DIR}" --lang cs --namespace "${NAMESPACE}" --project-path "${PROJECT_PATH}"

LINES="$(grep -r -o "namespace ${NAMESPACE}" "${TMP_DIR}" | wc -l | tr -d ' ')"
if [ "${LINES}" != 4 ] ; then
	echo "FOUND: ${LINES} EXPECTED: "
	exit 1
fi

LINES="$(grep -r -o "using SpacetimeDB;" "${TMP_DIR}" | wc -l | tr -d ' ')"
if [ "${LINES}" != 4 ] ; then
	echo "FOUND: ${LINES} EXPECTED: "
	exit 1
fi

rm -rf "${TMP_DIR}"
