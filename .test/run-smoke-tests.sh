#!/bin/bash

set -euo pipefail

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

cd "$(dirname "$0")"
source "lib.include"
cp ~/.spacetime/config.toml ~/.spacetime/.config.toml
cp ./config.toml ~/.spacetime/config.toml

cd ..
cargo build
export SPACETIME_HOME=$PWD

execute_test() {
	reset_config
	reset_project
	TEST_PATH=".test/tests/$1.sh"
	printf " **************** Running %s... " "$1"
	if ! bash -x "$TEST_PATH" > "$OUT_TMP" 2>&1 ; then
		printf "${RED}FAIL${CRST}\n"
		cat "$OUT_TMP"
		echo "Config file:"
		cat $HOME/.spacetime/config.toml
		failed_tests+=("$1")
	else 
		printf "${GRN}PASS${CRST}\n"
		passed_tests+=("$1")
	fi
}

TESTS="./.test/tests/*.sh"
if [ $# == 1 ] ; then
	if [ -f "./.test/tests/$1.sh" ]; then 
		execute_test "$1"
	else
		echo "Unknown test: $1"
		exit 1
	fi
elif [ $# == 0 ] ; then
	for smoke_test in $TESTS ; do
		execute_test "$(basename $smoke_test .sh)"
	done
else
	echo "Unknown parameters."
	exit 1
fi

cp ~/.spacetime/.config.toml ~/.spacetime/config.toml 

printf "\n\n*************************\n"
printf "** Smoke Tests Summary **\n"
printf "*************************\n\n"

printf "${GRN}Passed${CRST} Tests:\n"
for t in "${passed_tests[@]}" ; do 
	echo "	$t"
done

if [ ${#failed_tests[@]} -eq 0 ] ; then
	printf "\nNo failures reported.\n\n"
else
	printf "\n${RED}Failed${CRST} Tests:\n"
	for t in "${failed_tests[@]}" ; do 
		echo "	$t"
	done

	printf "\nDescriptions for failed tests:\n"
	for t in "${failed_tests[@]}" ; do 
		printf "\n$t\n\t"
		DESCRIBE_TEST=1 bash ".test/tests/${t}.sh"
	done
fi

rm -f "$OUT_TMP" "$TEST_OUT"
