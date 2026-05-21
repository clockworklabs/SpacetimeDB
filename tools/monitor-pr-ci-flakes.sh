#!/usr/bin/env bash

set -euo pipefail

TARGET="${TARGET:-20}"
POLL_SECONDS="${POLL_SECONDS:-300}"
STARTUP_SECONDS="${STARTUP_SECONDS:-60}"
TEST_SUITE_TIMEOUT_SECONDS="${TEST_SUITE_TIMEOUT_SECONDS:-5400}"
BATCH_FAILURE_MIN_TESTS="${BATCH_FAILURE_MIN_TESTS:-6}"
STATE_DIR="${STATE_DIR:-}"
RUN_ID="${RUN_ID:-}"
DRY_RUN=0

usage() {
	cat <<'EOF'
Usage: tools/monitor-pr-ci-flakes.sh [options]

Monitor the current branch PR's CI workflow until Smoketests and Test Suite
complete successfully TARGET times in a row, ignoring known non-actionable
leader-election batch failures and Test Suite timeouts/cancellations.

Options:
  --target N                         Consecutive successful runs required.
  --poll-seconds N                   Seconds between CI polls.
  --test-suite-timeout-seconds N     Treat running Test Suite jobs older than this as ignored.
  --state-dir PATH                   Directory for monitor state and downloaded logs.
  --run-id RUN_ID                    Seed monitoring from a specific CI run.
  --dry-run                          Validate context and classifiers without rerunning CI.
  -h, --help                         Show this help.

Environment variables with the same upper-case names may also be used.
EOF
}

while [[ "$#" -gt 0 ]]; do
	case "$1" in
		--target)
			shift
			TARGET="$1"
			;;
		--target=*)
			TARGET="${1#--target=}"
			;;
		--poll-seconds)
			shift
			POLL_SECONDS="$1"
			;;
		--poll-seconds=*)
			POLL_SECONDS="${1#--poll-seconds=}"
			;;
		--test-suite-timeout-seconds)
			shift
			TEST_SUITE_TIMEOUT_SECONDS="$1"
			;;
		--test-suite-timeout-seconds=*)
			TEST_SUITE_TIMEOUT_SECONDS="${1#--test-suite-timeout-seconds=}"
			;;
		--state-dir)
			shift
			STATE_DIR="$1"
			;;
		--state-dir=*)
			STATE_DIR="${1#--state-dir=}"
			;;
		--run-id)
			shift
			RUN_ID="$1"
			;;
		--run-id=*)
			RUN_ID="${1#--run-id=}"
			;;
		--dry-run)
			DRY_RUN=1
			;;
		-h|--help)
			usage
			exit 0
			;;
		*)
			echo "unknown argument: $1" >&2
			usage >&2
			exit 64
			;;
	esac
	shift
done

need() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "required command not found: $1" >&2
		exit 127
	fi
}

need date
need git
need gh
need jq
need notify-me
need rg

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

pr_json="$(gh pr view --json number,url,headRefName,headRefOid)"
pr_number="$(jq -r '.number' <<<"$pr_json")"
pr_url="$(jq -r '.url' <<<"$pr_json")"
branch="$(jq -r '.headRefName' <<<"$pr_json")"
head_sha="$(jq -r '.headRefOid' <<<"$pr_json")"

if [[ -z "$STATE_DIR" ]]; then
	STATE_DIR="/tmp/pr-ci-flake-loop-pr${pr_number}"
fi

LOG_DIR="$STATE_DIR/logs"
STATE_FILE="$STATE_DIR/state.tsv"
ACTIONABLE_FILE="$STATE_DIR/actionable.txt"
MONITOR_LOG="$STATE_DIR/monitor.log"

mkdir -p "$LOG_DIR"

log() {
	local msg="$*"
	printf '%s %s\n' "$(date -Is)" "$msg" | tee -a "$MONITOR_LOG"
}

notify() {
	local msg="$1"
	log "notify: $msg"
	if [[ "$DRY_RUN" == "1" ]]; then
		return 0
	fi
	notify-me "$msg"
}

load_state() {
	streak=0
	last_head_sha=""
	last_counted_key=""
	actionable_count=0
	ignored_count=0

	if [[ -f "$STATE_FILE" ]]; then
		while IFS=$'\t' read -r key value; do
			case "$key" in
				streak) streak="$value" ;;
				last_head_sha) last_head_sha="$value" ;;
				last_counted_key) last_counted_key="$value" ;;
				actionable_count) actionable_count="$value" ;;
				ignored_count) ignored_count="$value" ;;
			esac
		done < "$STATE_FILE"
	fi
}

save_state() {
	local tmp="$STATE_FILE.tmp"
	{
		printf 'streak\t%s\n' "$streak"
		printf 'last_head_sha\t%s\n' "$last_head_sha"
		printf 'last_counted_key\t%s\n' "$last_counted_key"
		printf 'actionable_count\t%s\n' "$actionable_count"
		printf 'ignored_count\t%s\n' "$ignored_count"
	} > "$tmp"
	mv "$tmp" "$STATE_FILE"
}

refresh_pr() {
	pr_json="$(gh pr view "$pr_number" --json number,url,headRefName,headRefOid)"
	branch="$(jq -r '.headRefName' <<<"$pr_json")"
	head_sha="$(jq -r '.headRefOid' <<<"$pr_json")"
}

latest_ci_run_for_head() {
	gh run list \
		--branch "$branch" \
		--limit 50 \
		--json databaseId,workflowName,headSha,createdAt,status,conclusion \
		--jq ".[] | select(.workflowName == \"CI\" and .headSha == \"$head_sha\") | .databaseId" \
		| head -n 1
}

wait_for_ci_run() {
	local run_id
	while true; do
		run_id="$(latest_ci_run_for_head)"
		if [[ -n "$run_id" ]]; then
			printf '%s\n' "$run_id"
			return 0
		fi
		log "no CI run visible for $branch@$head_sha; sleeping ${POLL_SECONDS}s"
		sleep "$POLL_SECONDS"
	done
}

target_jobs_json() {
	gh run view "$RUN_ID" --json jobs --jq '
		.jobs
		| map(select(.name == "Smoketests" or .name == "Test Suite"))
		| group_by(.name)
		| map(max_by(.startedAt))
	'
}

job_field() {
	local jobs="$1"
	local name="$2"
	local field="$3"
	jq -r ".[] | select(.name == \"$name\") | .$field // empty" <<<"$jobs"
}

epoch_seconds() {
	date -u -d "$1" +%s
}

download_log() {
	local name="$1"
	local job="$2"
	local safe_name="${name// /-}"
	local path="$LOG_DIR/${RUN_ID}-${safe_name}-${job}.log"
	gh run view "$RUN_ID" --job "$job" --log > "$path"
	printf '%s\n' "$path"
}

is_ignored_leader_batch_failure() {
	local path="$1"
	local leader_count failed_test_count failure_marker_count
	leader_count="$(rg -c "timeout waiting for (a )?leader" "$path" || true)"
	failed_test_count="$({ rg -o "test [^[:cntrl:]]+ \.\.\. FAILED|---- [^[:cntrl:]]+ stdout ----" "$path" || true; } | sort -u | wc -l | tr -d ' ')"
	failure_marker_count="$(rg -c "failures:|test result: FAILED| panicked at |FAILED" "$path" || true)"

	if [[ "$leader_count" -gt 0 && "$failed_test_count" -ge "$BATCH_FAILURE_MIN_TESTS" ]]; then
		return 0
	fi

	if [[ "$leader_count" -ge "$BATCH_FAILURE_MIN_TESTS" && "$failure_marker_count" -ge "$BATCH_FAILURE_MIN_TESTS" ]]; then
		return 0
	fi

	return 1
}

extract_failed_tests() {
	local path="$1"
	{
		rg -o "test [^[:cntrl:]]+ \.\.\. FAILED" "$path" | sed -E 's/^test //; s/ \.\.\. FAILED$//' || true
		rg -o "---- [^[:cntrl:]]+ stdout ----" "$path" | sed -E 's/^---- //; s/ stdout ----$//' || true
		rg -o "thread '\''[^'\'']+'\''" "$path" | sed -E "s/^thread '//; s/'$//" || true
	} | sort -u | paste -sd, -
}

rerun_workflow() {
	log "rerunning full CI workflow run $RUN_ID because both target jobs must be refreshed together"
	if [[ "$DRY_RUN" == "1" ]]; then
		return 0
	fi
	gh run rerun "$RUN_ID"
	sleep "$STARTUP_SECONDS"
}

cancel_run_for_timeout() {
	log "cancelling run $RUN_ID after Test Suite exceeded ${TEST_SUITE_TIMEOUT_SECONDS}s"
	if [[ "$DRY_RUN" != "1" ]]; then
		gh run cancel "$RUN_ID" || true
		gh run watch "$RUN_ID" --exit-status >/dev/null 2>&1 || true
	fi
}

record_actionable() {
	local job_name="$1"
	local job_id="$2"
	local log_path="$3"
	local tests="$4"

	actionable_count=$((actionable_count + 1))
	streak=0
	save_state

	{
		printf 'pr=%s\n' "$pr_number"
		printf 'run=%s\n' "$RUN_ID"
		printf 'job=%s\n' "$job_name"
		printf 'job_id=%s\n' "$job_id"
		printf 'tests=%s\n' "${tests:-unknown}"
		printf 'log=%s\n' "$log_path"
		printf 'streak_reset_to=0\n'
	} > "$ACTIONABLE_FILE"

	notify "PR #${pr_number}: actionable ${job_name} flake in run ${RUN_ID}; tests=${tests:-unknown}; log=${log_path}"
}

load_state

if [[ -n "$last_head_sha" && "$last_head_sha" != "$head_sha" ]]; then
	log "PR head changed from $last_head_sha to $head_sha; resetting streak"
	streak=0
	last_counted_key=""
fi
last_head_sha="$head_sha"
save_state

if [[ -z "$RUN_ID" ]]; then
	RUN_ID="$(wait_for_ci_run)"
fi

log "monitoring PR #${pr_number} ($pr_url) branch=$branch head=$head_sha run=$RUN_ID target=$TARGET"

if [[ "$DRY_RUN" == "1" ]]; then
	jobs="$(target_jobs_json)"
	log "dry run target jobs: $(jq -c '[.[] | {name, databaseId, status, conclusion, startedAt, completedAt}]' <<<"$jobs")"
	log "dry run state dir: $STATE_DIR"
	exit 0
fi

while [[ "$streak" -lt "$TARGET" ]]; do
	refresh_pr
	if [[ "$head_sha" != "$last_head_sha" ]]; then
		log "PR head changed from $last_head_sha to $head_sha; resetting streak and switching runs"
		streak=0
		last_counted_key=""
		last_head_sha="$head_sha"
		save_state
		RUN_ID="$(wait_for_ci_run)"
	fi

	jobs="$(target_jobs_json)"
	target_count="$(jq 'length' <<<"$jobs")"
	if [[ "$target_count" -lt 2 ]]; then
		run_status="$(gh run view "$RUN_ID" --json status,conclusion --jq '.status + " " + (.conclusion // "")')"
		log "target jobs not both visible for run $RUN_ID ($run_status); sleeping ${POLL_SECONDS}s"
		sleep "$POLL_SECONDS"
		continue
	fi

	all_success=1
	any_running=0
	any_failed=0
	failed_names=()
	failed_jobs=()

	for name in "Smoketests" "Test Suite"; do
		job="$(job_field "$jobs" "$name" databaseId)"
		status="$(job_field "$jobs" "$name" status)"
		conclusion="$(job_field "$jobs" "$name" conclusion)"
		started_at="$(job_field "$jobs" "$name" startedAt)"
		completed_at="$(job_field "$jobs" "$name" completedAt)"
		log "$name job=$job status=$status conclusion=${conclusion:-none} started=${started_at:-none} completed=${completed_at:-none}"

		if [[ "$name" == "Test Suite" && "$status" != "completed" && -n "$started_at" ]]; then
			now="$(date -u +%s)"
			started_epoch="$(epoch_seconds "$started_at")"
			age=$((now - started_epoch))
			if [[ "$age" -gt "$TEST_SUITE_TIMEOUT_SECONDS" ]]; then
				ignored_count=$((ignored_count + 1))
				save_state
				log "Test Suite has run for ${age}s; treating as ignored timeout"
				cancel_run_for_timeout
				rerun_workflow
				RUN_ID="$(wait_for_ci_run)"
				continue 2
			fi
		fi

		if [[ "$status" != "completed" ]]; then
			any_running=1
			all_success=0
		elif [[ "$conclusion" == "success" ]]; then
			:
		else
			any_failed=1
			all_success=0
			failed_names+=("$name")
			failed_jobs+=("$job")
		fi
	done

	if [[ "$all_success" -eq 1 ]]; then
		smoke_job="$(job_field "$jobs" "Smoketests" databaseId)"
		test_job="$(job_field "$jobs" "Test Suite" databaseId)"
		success_key="Smoketests:${smoke_job}|Test Suite:${test_job}"

		if [[ "$success_key" != "$last_counted_key" ]]; then
			streak=$((streak + 1))
			last_counted_key="$success_key"
			save_state
			log "cycle passed; streak=$streak/$TARGET"
		else
			log "successful job pair already counted; streak remains $streak/$TARGET"
		fi

		if [[ "$streak" -ge "$TARGET" ]]; then
			notify "PR #${pr_number}: did not see an actionable Smoketests/Test Suite flake in ${TARGET} consecutive runs."
			log "success: reached $TARGET consecutive successful target runs"
			exit 0
		fi

		rerun_workflow
		RUN_ID="$(wait_for_ci_run)"
		continue
	fi

	if [[ "$any_running" -eq 1 && "$any_failed" -eq 0 ]]; then
		log "target jobs still running; sleeping ${POLL_SECONDS}s"
		sleep "$POLL_SECONDS"
		continue
	fi

	if [[ "$any_running" -eq 1 && "$any_failed" -eq 1 ]]; then
		log "one target failed while another is still running; sleeping ${POLL_SECONDS}s before classification"
		sleep "$POLL_SECONDS"
		continue
	fi

	if [[ "$any_failed" -eq 1 ]]; then
		ignored_this_failure=1
		actionable_seen=0

		for i in "${!failed_jobs[@]}"; do
			name="${failed_names[$i]}"
			job="${failed_jobs[$i]}"
			conclusion="$(job_field "$jobs" "$name" conclusion)"

			if [[ "$name" == "Test Suite" ]] && [[ "$conclusion" == "cancelled" || "$conclusion" == "timed_out" ]]; then
				log "ignoring Test Suite conclusion=$conclusion"
				continue
			fi

			path="$(download_log "$name" "$job")"
			log "downloaded $name log to $path"

			if is_ignored_leader_batch_failure "$path"; then
				log "ignoring leader-election batch failure in $name"
				continue
			fi

			ignored_this_failure=0
			actionable_seen=1
			tests="$(extract_failed_tests "$path")"
			record_actionable "$name" "$job" "$path" "$tests"
		done

		if [[ "$ignored_this_failure" -eq 1 ]]; then
			ignored_count=$((ignored_count + 1))
			save_state
			log "ignored failure; streak remains $streak/$TARGET"
			rerun_workflow
			RUN_ID="$(wait_for_ci_run)"
			continue
		fi

		if [[ "$actionable_seen" -eq 1 ]]; then
			log "actionable failure recorded; stopping monitor"
			exit 0
		fi
	fi

	log "unknown target state; sleeping ${POLL_SECONDS}s"
	sleep "$POLL_SECONDS"
done
