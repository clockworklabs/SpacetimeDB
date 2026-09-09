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
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::Internal, reducer.visibility);
            found = true;
        }
    }
    ASSERT_TRUE(found);
}

TEST_CASE(v10_retains_explicit_visibility_and_schedule_default) {
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
    versioned.set<2>(builder.BuildModuleDef());
    std::vector<uint8_t> bytes;
    bsatn::Writer writer(bytes);
    bsatn::serialize(writer, versioned);
    ASSERT_EQ(uint8_t{2}, bytes.at(0));
    ASSERT_EQ(uint8_t{2}, versioned.get_tag());
    bool saw_reducers = false, saw_procedure = false, saw_environment = false, saw_capability = false;
    for (const auto& section : versioned.get<2>().sections) {
        if (section.get_tag() == 3) {
            const auto& reducers = section.get<3>();
            ASSERT_EQ(size_t{4}, reducers.size());
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::ClientCallable, reducers[0].visibility);
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::ExplicitClientCallable, reducers[1].visibility);
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::Private, reducers[2].visibility);
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::Internal, reducers[3].visibility);
            saw_reducers = true;
        } else if (section.get_tag() == 4) {
            ASSERT_EQ(SpacetimeDB::Internal::FunctionVisibility::Internal, section.get<4>().at(0).visibility);
            saw_procedure = true;
        } else if (section.get_tag() == 15) {
            ASSERT_TRUE(section.get<15>().empty());
            saw_environment = true;
        } else if (section.get_tag() == 16) {
            ASSERT_EQ(std::vector<std::string>{"hosted_auth_v1"}, section.get<16>());
            saw_capability = true;
        }
    }
    ASSERT_TRUE(saw_reducers && saw_procedure && saw_environment && saw_capability);
}

TEST_CASE(v10_visibility_extends_enum_without_changing_reducer_field_layout) {
    V10Builder builder;
    builder.RegisterReducer("r", &noop, {});
    auto reducer = builder.GetReducers().at(0);
    for (uint8_t tag = 0; tag <= 3; ++tag) {
        reducer.visibility = static_cast<SpacetimeDB::Internal::FunctionVisibility>(tag);
        std::vector<uint8_t> bytes;
        bsatn::Writer writer(bytes);
        bsatn::serialize(writer, reducer);
        const std::vector<uint8_t> expected{1, 0, 0, 0, 'r', 0, 0, 0, 0, tag, 2, 0, 0, 0, 0, 4};
        ASSERT_EQ(expected, bytes);
        const RawProcedureDefV10 procedure_def{
            "p", ProductType{}, reducer.ok_return_type, reducer.visibility,
        };
        std::vector<uint8_t> procedure_bytes;
        bsatn::Writer procedure_writer(procedure_bytes);
        bsatn::serialize(procedure_writer, procedure_def);
        const std::vector<uint8_t> expected_procedure{
            1, 0, 0, 0, 'p', 0, 0, 0, 0, 2, 0, 0, 0, 0, tag,
        };
        ASSERT_EQ(expected_procedure, procedure_bytes);
    }
}

TEST_CASE(v10_environment_and_capabilities_have_distinct_appended_wire_tags) {
    RawModuleDefV10Section environment;
    environment.set<15>(std::vector<EnvironmentDeclaration>{});
    RawModuleDefV10Section capabilities;
    capabilities.set<16>(std::vector<std::string>{});
    for (const auto& section : {environment, capabilities}) {
        std::vector<uint8_t> bytes;
        bsatn::Writer writer(bytes);
        bsatn::serialize(writer, section);
        const std::vector<uint8_t> expected{section.get_tag(), 0, 0, 0, 0};
        ASSERT_EQ(expected, bytes);
    }
}
