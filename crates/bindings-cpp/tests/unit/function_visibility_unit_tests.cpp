#include "test_harness.h"
#include "spacetimedb/reducer_error.h"
#include "spacetimedb/procedure_context.h"
#include "spacetimedb/internal/v10_builder.h"
#include "spacetimedb/internal/autogen/RawModuleDef.g.h"
#include "spacetimedb/macros.h"

using namespace SpacetimeDB;
using namespace SpacetimeDB::Internal;

namespace {
ReducerResult noop(ReducerContext) { return Ok(); }
uint32_t procedure(ProcedureContext) { return 7; }
}

SPACETIMEDB_FUNCTION_VISIBILITY(visibility_macro_target, Internal);

TEST_CASE(visibility_macro_applies_after_function_registration) {
    auto& builder = getV10Builder();
    builder.RegisterReducer("visibility_macro_target", &noop, {});
    __spacetimedb_function_visibility_visibility_macro_target();
    bool found = false;
    for (const auto& section : builder.BuildModuleDef().sections) {
        if (section.get_tag() != 3) continue;
        for (const auto& reducer : section.get<3>()) {
            if (reducer.source_name != "visibility_macro_target") continue;
            ASSERT_EQ(FunctionVisibilityV11::Internal, *reducer.declared_visibility);
            found = true;
        }
    }
    ASSERT_TRUE(found);
}

TEST_CASE(v11_retains_explicit_visibility_and_schedule_default_omission) {
    V10Builder builder;
    builder.RegisterReducer("omitted", &noop, {});
    builder.RegisterReducer("public", &noop, {});
    builder.RegisterReducer("private", &noop, {});
    builder.RegisterReducer("internal", &noop, {});
    builder.SetFunctionVisibility("public", SpacetimeDB::FunctionVisibility::Public);
    builder.SetFunctionVisibility("private", SpacetimeDB::FunctionVisibility::Private);
    builder.SetFunctionVisibility("internal", SpacetimeDB::FunctionVisibility::Internal);
    builder.RegisterSchedule("jobs", 0, "public");
    builder.RegisterSchedule("other_jobs", 0, "omitted");
    builder.RegisterProcedure("procedure", &procedure);
    builder.SetFunctionVisibility("procedure", SpacetimeDB::FunctionVisibility::Internal);

    RawModuleDef versioned;
    versioned.set<3>(builder.BuildModuleDef());
    std::vector<uint8_t> bytes;
    bsatn::Writer writer(bytes);
    bsatn::serialize(writer, versioned);
    ASSERT_EQ(uint8_t{3}, bytes.at(0));
    ASSERT_EQ(uint8_t{3}, versioned.get_tag());
    bool saw_reducers = false, saw_procedure = false, saw_capability = false;
    for (const auto& section : versioned.get<3>().sections) {
        if (section.get_tag() == 3) {
            const auto& reducers = section.get<3>();
            ASSERT_EQ(size_t{4}, reducers.size());
            ASSERT_TRUE(!reducers[0].declared_visibility.has_value());
            ASSERT_EQ(FunctionVisibilityV11::ClientCallable, *reducers[1].declared_visibility);
            ASSERT_EQ(FunctionVisibilityV11::Private, *reducers[2].declared_visibility);
            ASSERT_EQ(FunctionVisibilityV11::Internal, *reducers[3].declared_visibility);
            saw_reducers = true;
        } else if (section.get_tag() == 4) {
            ASSERT_EQ(FunctionVisibilityV11::Internal, *section.get<4>().at(0).declared_visibility);
            saw_procedure = true;
        } else if (section.get_tag() == 13) {
            ASSERT_EQ(std::vector<std::string>{"hosted_auth_v1"}, section.get<13>());
            saw_capability = true;
        }
    }
    ASSERT_TRUE(saw_reducers && saw_procedure && saw_capability);
}
