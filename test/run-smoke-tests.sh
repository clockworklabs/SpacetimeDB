#!/bin/bash

set -euo pipefail

# set -x

cd "$(dirname "$0")"

SPACETIME_CARGO_PROFILE=release-fast
CRST='\033[0m'		 # Text Reset
GRN='\033[0;32m'	 # Green
RED='\033[0;31m'	 # Red
passed_tests=()
failed_tests=()
RESET_SPACETIME_CONFIG=$(mktemp)
export RESET_SPACETIME_CONFIG
export SPACETIME_DIR="$PWD/.."
RUN_PARALLEL=false

declare -a TESTS
for test in tests/*.sh ; do
	file_name="$(basename "$test")"
	TESTS+=("${file_name%.*}")
done

EXCLUDE_TESTS=()

while [ $# != 0 ] ; do
	case $1 in
		-x)
			shift
			EXCLUDE_TESTS+=("$@")
			break
		;;
		--parallel)
			shift
			RUN_PARALLEL=true
			echo "Running tests in parallel."
		;;
		*)
			TESTS=("$@")
			break
		;;
	esac
done

rustup update $(rustup show active-toolchain | cut -d' ' -f1)
rustup component add clippy

source "lib.include"
cp ./config.toml "$RESET_SPACETIME_CONFIG"

cd ..
export SPACETIME_HOME=$PWD

# Create a project that we can copy to reset our project
RESET_PROJECT_PATH=$(mktemp -d)
export RESET_PROJECT_PATH
cargo run init "$RESET_PROJECT_PATH" --lang rust
# We have to force using the local spacetimedb_bindings otherwise we will download them from crates.io
if [[ "$OSTYPE" == "darwin"* ]]; then
	sed -i '' "s@.*spacetimedb.*=.*@spacetimedb = { path = \"${SPACETIME_DIR}/crates/bindings\" }@g" "${RESET_PROJECT_PATH}/Cargo.toml"
elif [[ "$OSTYPE" == "msys"* ]]; then
	# Running in git bash; do horrible path conversion; yes we do need all of those
	WINPATH="$(cygpath -w "${SPACETIME_DIR}/crates/bindings" | sed 's/\\/\\\\\\\\/g')"
	sed -i "s@.*spacetimedb.*=.*@spacetimedb = { path = \"${WINPATH}\" }@g" "${RESET_PROJECT_PATH}/Cargo.toml"
else
	sed -i "s@.*spacetimedb.*=.*@spacetimedb = { path = \"${SPACETIME_DIR}/crates/bindings\" }@g" "${RESET_PROJECT_PATH}/Cargo.toml"
fi

cargo run build "$RESET_PROJECT_PATH" -s -d

export SPACETIME_SKIP_CLIPPY=1

if [ -z "${NO_DOCKER:-}" ] ; then
	if [ "$(docker ps | grep "node" -c)" != 1 ] ; then
		echo "Docker container not found, is SpacetimeDB running?"
		exit 1
	fi

	CONTAINER_NAME=$(docker ps | grep "node" | awk '{print $NF}')
	docker logs "$CONTAINER_NAME"
fi

execute_procedural_test() {
	if [ $# != 1 ] ; then
		echo "Usage: execute_procedural_test <test-name>"
		exit 1
	fi

	test_name=$1

	reset_config
	reset_project

	test_out_file=$(mktemp)
	printf " **************** Running %s... " "$test_name"
	execute_test "$test_name" "$test_out_file"
	result_code=$?
	set -e
	if [ $result_code == 0 ] ; then
		printf "${GRN}PASS${CRST}\n"
	else
		printf "${RED}FAIL${CRST}\n"
	fi
	process_test_result "$test_name" "$result_code" "$PROJECT_PATH" "$test_out_file" "$SPACETIME_CONFIG_FILE"
}

# Note: $PROJECT_PATH and $SPACETIME_CONFIG_FILE are implicit environment variable inputs to this function.
# Before calling this function you should have already run `reset_project` and `reset_config` yourself.
execute_test() {
	if [ "$#" != 2 ] ; then
		echo "Usage: execute_test <test-name> <test-out-file>"
		exit 1
	fi
	[ -d "$PROJECT_PATH" ]
	[ -f "$SPACETIME_CONFIG_FILE" ]

	test_name=$1
	test_path="test/tests/$test_name.sh"
	test_out_file=$2

	# TODO: Remove this, we shouldn't be using this TEST_OUT variable anymore because it
	# basically has no function. We should just be using piping where needed
	TEST_OUT=$(mktemp)
	export TEST_OUT
	# end todo

	set +e
	bash -x "$test_path" > "$test_out_file" 2>&1
	result_code=$?
	set -e
	echo "Test Out:" >> "$test_out_file"
	cat "$TEST_OUT" >> "$test_out_file"

	rm -f "$TEST_OUT"
	# if ! bash -x "$test_path" ; then
	set +e
	return $result_code
}

# Prints the result of a test. If the test failed, then the test output is printed.
process_test_result() {
	if [ $# != 5 ] ; then
		echo "Usage: process_test_result <test-name> <result-code> <project-path> <out-file-path> <config-file-path>"
		exit 1
	fi

	test_name=$1
	result_code=$2
	PROJECT_PATH=$3
	out_file_path=$4
	config_file_path=$5

	if [ "$result_code" == 0 ] ; then
		passed_tests+=("$test_name")

		# Cleanup the test execution only if the test passed
		rm -rf "$PROJECT_PATH" "$out_file_path" "$config_file_path"
	else
		[ -z "${NO_DOCKER:-}" ] && docker logs "$CONTAINER_NAME"
		cat "$out_file_path"
		echo "Config file:"
		cat "$config_file_path"
		echo "PROJECT_PATH=$PROJECT_PATH TEST_OUT=$out_file_path SPACETIME_CONFIG_FILE=$config_file_path"
		failed_tests+=("$test_name")
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


# Arrays used for running tests procedurally
TESTS_PID=()
TESTS_OUT_FILE=()
TESTS_NAME=()
TESTS_PROJECT_PATH=()
TESTS_CONFIG_FILE=()

# Make sure background tests are torn down even if we don't get to `wait`
# for them (e.g. on ^C). Mainly for $RUN_PARALLEL.
terminate_jobs() {
	local running=""
	running="$(jobs -pr)"
	if [ -n "$running" ]; then
		kill "$running"
	fi
}
trap 'terminate_jobs' SIGINT SIGTERM EXIT

for smoke_test in "${TESTS[@]}" ; do
	if [ ${#EXCLUDE_TESTS[@]} -ne 0 ] && list_contains "$smoke_test" "${EXCLUDE_TESTS[@]}"; then
		echo "Skipping test $smoke_test"
		continue
	fi
	if [ -f "./test/tests/$smoke_test.sh" ]; then
		if [ "$RUN_PARALLEL" == "true" ] ; then
			# Skip any non-parallizable tests if we're running in parallel
			if [[ "$smoke_test" == zz_* ]] ; then
				continue
			fi

			TESTS_NAME+=("$smoke_test")
			reset_config
			TESTS_CONFIG_FILE+=("$SPACETIME_CONFIG_FILE")

			reset_project
			TESTS_PROJECT_PATH+=("$PROJECT_PATH")

			test_out_file=$(mktemp)
			TESTS_OUT_FILE+=("$test_out_file")

			(execute_test "$smoke_test" "$test_out_file") &
			TESTS_PID+=($!)
			echo "[PARALLEL] Test started: $smoke_test"
		else
			execute_procedural_test "$smoke_test"
		fi
	else
		echo "Unknown test: $smoke_test"
		exit 1
	fi
done


if [ "$RUN_PARALLEL" == "true" ] ; then
	# Wait for all processes to end, and save their exit codes
	length=${#TESTS_PID[@]}
	while true ; do
		FOUND=0
		for ((i=0; i<length; i++)) ; do
			pid=${TESTS_PID[$i]}
			if [ "$pid" == "" ] ; then
				continue
			fi
			FOUND=1

			# If the process is still running, skip it
			if kill -0 "$pid" 2>/dev/null; then
				continue
			fi

			out_file=${TESTS_OUT_FILE[$i]}
			test_name=${TESTS_NAME[$i]}
			project_path=${TESTS_PROJECT_PATH[$i]}
			config_file=${TESTS_CONFIG_FILE[$i]}
			set +e
			wait "$pid"
			result_code=$?
			set -e
			if [ $result_code == 0 ] ; then
				printf "[${GRN}PASS${CRST}] | $test_name finished\n"
			else
				printf "[${RED}FAIL${CRST}] | $test_name finished\n"
			fi
			process_test_result "$test_name" "$result_code" "$project_path" "$out_file" "$config_file"
			TESTS_PID[i]=""
		done

		if [ $FOUND == 0 ] ; then
			break;
		fi
	done

	# Now run any tests that cannot be parallelized
	for smoke_test in "${TESTS[@]}" ; do
		if [ ${#EXCLUDE_TESTS[@]} -ne 0 ] && list_contains "$smoke_test" "${EXCLUDE_TESTS[@]}"; then
			continue
		fi

		if [[ "$smoke_test" == zz_* ]]; then
			if [ -f "./test/tests/$smoke_test.sh" ]; then
				execute_procedural_test "$smoke_test"
			else
				echo "Unknown test: $smoke_test"
				exit 1
			fi
		fi

	done
fi

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

# vim: noexpandtab tabstop=4 shiftwidth=4
