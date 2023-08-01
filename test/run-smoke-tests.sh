#!/bin/bash

set -euo pipefail
set -x
cd "$(dirname "$0")"

CRST='\033[0m'       # Text Reset
GRN='\033[0;32m'     # Green
RED='\033[0;31m'     # Red
passed_tests=()
failed_tests=()
OUT_TMP=$(mktemp)
TEST_OUT=$(mktemp)
export TEST_OUT
PROJECT_PATH=$(mktemp -d)
export PROJECT_PATH
export SPACETIME_DIR="$PWD/.."

export SPACETIME_SKIP_CLIPPY=1
CONTAINER_NAME=$(docker ps | grep node | awk '{print $NF}')
docker logs "$CONTAINER_NAME"

source "lib.include"
mkdir -p ~/.spacetime
if [ -f ~/.spacetime/config.toml ] ; then
	cp ~/.spacetime/config.toml ~/.spacetime/.config.toml
fi
cp ./config.toml ~/.spacetime/config.toml

cd ..
cargo build
export SPACETIME_HOME=$PWD

execute_test() {
	reset_config
	reset_project
	TEST_PATH="test/tests/$1.sh"
	printf " **************** Running %s... " "$1"
	if ! bash -x "$TEST_PATH" > "$OUT_TMP" 2>&1 ; then
		printf "${RED}FAIL${CRST}\n"
		cat "$OUT_TMP"
		echo "Config file:"
		cat "$HOME/.spacetime/config.toml"
		docker logs "$CONTAINER_NAME"
		failed_tests+=("$1")
	else 
		printf "${GRN}PASS${CRST}\n"
		passed_tests+=("$1")
	fi
}

list_contains() {
	local a=$1
	shift
	for x in "$@"; do
		if [[ "$x" == "$a" ]]; then
			return 0
		fi
	done
	return 1
}

TESTS=(./test/tests/*.sh)
TESTS=("${TESTS[@]#./test/tests/}")
TESTS=("${TESTS[@]%.sh}")

EXCLUDE_TESTS=()

if [ $# != 0 ] ; then
	case $1 in
		-x)
			shift
			EXCLUDE_TESTS+=("$@")
		;;
		*)
			TESTS=("$@")
		;;
	esac
fi

for smoke_test in "${TESTS[@]}" ; do
	if [ ${#EXCLUDE_TESTS[@]} -ne 0 ] && list_contains "$smoke_test" "${EXCLUDE_TESTS[@]}"; then
		continue
	fi
	if [ -f "./test/tests/$smoke_test.sh" ]; then
		execute_test "$smoke_test"
	else
		echo "Unknown test: $smoke_test"
		exit 1
	fi
done

if [ -f ~/.spacetime/.config.toml ] ; then
	cp ~/.spacetime/.config.toml ~/.spacetime/config.toml 
fi

rm -f "$OUT_TMP" "$TEST_OUT"

printf "\n\n*************************\n"
printf "** Smoke Tests Summary **\n"
printf "*************************\n\n"

if [ ${#passed_tests[@]} -ne 0 ]; then
	printf "${GRN}Passed${CRST} Tests:\n"
	for t in "${passed_tests[@]}" ; do
		echo "	$t"
	done
fi

if [ ${#failed_tests[@]} -eq 0 ] ; then
	printf "\nNo failures reported.\n\n"
else
	printf "\n${RED}Failed${CRST} Tests:\n"
	for t in "${failed_tests[@]}" ; do 
		echo "	$t"
	done

	printf "\nDescriptions for failed tests:\n"
	for t in "${failed_tests[@]}" ; do 
		printf "\n%s\n\t" "$t"
		DESCRIBE_TEST=1 bash "test/tests/${t}.sh"
	done

	exit 1
fi
