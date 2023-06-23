#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")"

CRST='\033[0m'       # Text Reset
GRN='\033[0;32m'     # Green
RED='\033[0;31m'     # Red
passed_tests=()
failed_tests=()
OUT_TMP=$(mktemp)
TEST_OUT=$(mktemp)
export TEST_OUT
RESET_SPACETIME_CONFIG=$(mktemp)
export RESET_SPACETIME_CONFIG
PROJECT_PATH=$(mktemp -d)
export PROJECT_PATH
export SPACETIME_DIR="$PWD/.."
SPACETIME_CONFIG_FILE=$(mktemp)
export SPACETIME_CONFIG_FILE
RUN_PARALLEL=false

export SPACETIME_SKIP_CLIPPY=1
CONTAINER_NAME=$(docker ps | grep node | awk '{print $NF}')
docker logs "$CONTAINER_NAME"

rustup update
rustup component add clippy

source "lib.include"
cp ./config.toml "$RESET_SPACETIME_CONFIG"

cd ..
cargo build
export SPACETIME_HOME=$PWD

execute_test() {
    reset_test_out
	reset_config
	reset_project
    echo "TEST_OUT=$TEST_OUT PROJECT_PATH=$PROJECT_PATH SPACETIME_CONFIG_FILE=$SPACETIME_CONFIG_FILE"
	TEST_PATH="test/tests/$1.sh"
	printf " **************** Running %s... " "$1"
    RETURN_CODE=0
	if ! bash -x "$TEST_PATH" > "$OUT_TMP" 2>&1 ; then
		printf "${RED}FAIL${CRST}\n"
		cat "$OUT_TMP"
		echo "Config file:"
        cat "$SPACETIME_CONFIG_FILE"
		# docker logs "$CONTAINER_NAME"
		failed_tests+=("$1")

        if [ "$RUN_PARALLEL" == "true" ] ; then
            RETURN_CODE=1
        fi
	else
		printf "${GRN}PASS${CRST}\n"
		passed_tests+=("$1")
	fi

    rm -rf "$PROJECT_PATH" "$TEST_OUT" "$SPACETIME_CONFIG_FILE"
    if [[ $RETURN_CODE != 0 ]] ; then
        echo "Returning non-zero exit code!"
        exit 1
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

# TODO: Remove this!
# TESTS=("upload-module-1" "upload-module-1")

EXCLUDE_TESTS=()

if [ $# != 0 ] ; then
	case $1 in
		-x)
			shift
			EXCLUDE_TESTS+=("$@")
		;;
        --parallel)
            shift
            RUN_PARALLEL=true
        ;;
		*)
			TESTS=("$@")
		;;
	esac
fi


TESTS_PID=()
TESTS_OUT=()
TESTS_NAME=()
for smoke_test in "${TESTS[@]}" ; do
	if [ ${#EXCLUDE_TESTS[@]} -ne 0 ] && list_contains "$smoke_test" "${EXCLUDE_TESTS[@]}"; then
		continue
	fi
	if [ -f "./test/tests/$smoke_test.sh" ]; then
        if [ "$RUN_PARALLEL" == "true" ] ; then
            # Skip any non-parallizable tests if we're running in parallel
            if [[ "$smoke_test" == zz_* ]] ; then
                continue
            fi

            process_output_file=$(mktemp)
            (execute_test "$smoke_test" > "$process_output_file") &
            TESTS_PID+=($!)
            TESTS_OUT+=("$process_output_file")
            TESTS_NAME+=("$smoke_test")
        else
		    execute_test "$smoke_test"
        fi
	else
		echo "Unknown test: $smoke_test"
		exit 1
	fi
done


if [ "$RUN_PARALLEL" == "true" ] ; then
    # Wait for all processes to end, and save their exit codes
    length=${#TESTS_PID[@]}
    for ((i=0; i<length; i++)) ; do
        pid=${TESTS_PID[$i]}
        out_file=${TESTS_OUT[$i]}
        test_name=${TESTS_NAME[$i]}
        set +e
        wait "$pid"
        RESULT_CODE=$?
        set -e
        echo "Process result code: $RESULT_CODE"
        if [ $RESULT_CODE == 0 ] ; then
            cat "$out_file"
		    passed_tests+=("$test_name")
        else
            echo "+------------------------------------------+"
            printf "$RED TEST FAILURE:$CRST $test_name\n"
            echo
            cat "$out_file"
		    failed_tests+=("$test_name")
        fi
        echo "Process finished: $pid"
    done
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
