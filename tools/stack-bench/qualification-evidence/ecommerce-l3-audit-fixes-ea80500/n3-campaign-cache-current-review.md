# Current campaign-cache check

Current 27e01bd86a: both cache tests pass on Windows (18.35 seconds) and isolated Linux (12.92 seconds). Linux exit0, OOMKilled=false. No failure/cancellation/skip.

Linux image: sha256:8ab84965ad52561b203a7bab06d1020c3b3b434a061657ad5a94f923ad17b10c. The run used two CPUs, 4 GiB container memory, default Node heap settings, no network, no credentials, and a read-only bind of repository tests because the runtime image omits source fixtures. Command: node --test --test-timeout=120000 dist/tests/dashboard/campaign-cache.test.js. Test temp data stayed in its isolated container.

The first Linux invocation omitted that fixture mount. It failed ENOENT before either cache test exercised its behavior. Its log and container state are preserved, not counted as a product regression.

The earlier audit-fixes-linux-serial-2.log has 43 passing selected tests and includes both cache tests. Its exact launch flags were not found in the saved note/script search. The external review gives no exact memory/Node flags for its historical comparison. Thus equivalent conditions to that historical OOM are not established. Current standalone passing evidence does not disprove a historical regression, nor prove memory use cannot grow. It does show that this failure does not reproduce in the documented current Linux environment. No source fix is justified by this current check alone.

Latest review's new N3 statement specifically distinguishes campaign-cache OOM at4f from a passing37 comparison, while labeling depth-pause/dashboard hangs pre-existing. I have no preserved byte-exact11:53 copy, so cannot give a complete textual diff. Previous saved review notes already mention the historical4f OOM; the new before/after attribution is the material addition.

Evidence: n3-campaign-cache-current-verification.json records exact controller states/IDs, command, resource bounds, source/fixture/log hashes. Both owned test containers are stopped and contain no backend resources. Parent may remove only those exact controllers after preserving this evidence. No containers were removed here.
