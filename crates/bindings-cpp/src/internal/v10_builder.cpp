#include "spacetimedb/environment.h"
#include "spacetimedb/internal/v10_builder.h"
#include "spacetimedb/internal/autogen/AlgebraicType.g.h"
#include "spacetimedb/internal/autogen/ProductType.g.h"
#include "spacetimedb/internal/autogen/ProductTypeElement.g.h"
#include "spacetimedb/internal/autogen/RawModuleDefV10Section.g.h"
#include "spacetimedb/internal/autogen/RawTypeDefV10.g.h"
#include "spacetimedb/internal/autogen/RawScopedTypeNameV10.g.h"
#include "spacetimedb/internal/autogen/FunctionVisibility.g.h"
#include "spacetimedb/internal/autogen/ExplicitNames.g.h"
#include "spacetimedb/router.h"
#include <algorithm>
#include <unordered_set>

namespace SpacetimeDB {
namespace Internal {

std::unique_ptr<V10Builder> g_v10_builder;

void initializeV10Builder() {
    g_v10_builder = std::make_unique<V10Builder>();
}

V10Builder& getV10Builder() {
    if (!g_v10_builder) {
        initializeV10Builder();
    }
    return *g_v10_builder;
}

void V10Builder::Clear() {
    typespace_ = Typespace{};
    types_.clear();
    table_is_event_.clear();
    case_conversion_policy_.reset();
    explicit_names_.clear();
    column_defaults_by_table_.clear();
    tables_.clear();
    reducers_.clear();
    procedures_.clear();
    views_.clear();
    view_primary_keys_.clear();
    http_handlers_.clear();
    http_routes_.clear();
    schedules_.clear();
    lifecycle_reducers_.clear();
    row_level_security_.clear();
    outbox_registrations_.clear();
    outboxes_.clear();
}

V10Builder::OutboxRegistration* V10Builder::FindOutbox(const std::string& table_name) {
    for (auto& outbox : outbox_registrations_) {
        if (outbox.table_name == table_name) {
            return &outbox;
        }
    }
    return nullptr;
}

void V10Builder::SetOutboxTarget(const std::string& table_name, const std::string& field_name) {
    if (auto* outbox = FindOutbox(table_name)) {
        outbox->target_fields.push_back(field_name);
    }
}

void V10Builder::AddOutboxParamAlias(const std::string& table_name,
                                     const std::string& field_name,
                                     const std::string& param_name) {
    if (auto* outbox = FindOutbox(table_name)) {
        outbox->param_aliases.emplace_back(field_name, param_name);
    }
}

void V10Builder::SetOutboxOnResult(const std::string& table_name, const std::string& reducer_name) {
    if (auto* outbox = FindOutbox(table_name)) {
        outbox->on_result_reducers.push_back(reducer_name);
    }
}

namespace {

std::optional<U256> DecodeSignatureHash(const std::string& hex) {
    if (hex.size() != 64) {
        return std::nullopt;
    }
    auto nibble = [](char c) -> int {
        if (c >= '0' && c <= '9') return c - '0';
        if (c >= 'a' && c <= 'f') return c - 'a' + 10;
        if (c >= 'A' && c <= 'F') return c - 'A' + 10;
        return -1;
    };
    // The hex string lists the hash's bytes in order, which is the U256's little-endian encoding.
    uint8_t bytes[32];
    for (size_t i = 0; i < 32; ++i) {
        int high = nibble(hex[2 * i]);
        int low = nibble(hex[2 * i + 1]);
        if (high < 0 || low < 0) {
            return std::nullopt;
        }
        bytes[i] = static_cast<uint8_t>((high << 4) | low);
    }
    return U256(bytes);
}

} // namespace

void V10Builder::ResolveOutboxes() {
    outboxes_.clear();
    const auto u64_type = bsatn::to_bytes(AlgebraicType(AlgebraicType::Tag::U64));
    for (const auto& outbox : outbox_registrations_) {
        const std::string& table = outbox.table_name;
        auto fail = [&](const std::string& code, const std::string& details) {
            SetConstraintRegistrationError(code + "_" + table, "outbox table='" + table + "': " + details);
        };
        auto column_index = [&](const std::string& name) -> std::optional<uint16_t> {
            for (uint16_t i = 0; i < outbox.column_names.size(); ++i) {
                if (outbox.column_names[i] == name) {
                    return i;
                }
            }
            return std::nullopt;
        };

        auto table_it = FindTable(table);
        if (table_it == tables_.end()) {
            fail("OUTBOX_TABLE_NOT_FOUND", "the table was not registered");
            return;
        }

        // The message ID: a single auto-incrementing u64 primary key.
        bool has_message_id = table_it->primary_key.size() == 1 &&
                              outbox.column_types[table_it->primary_key[0]] == u64_type &&
                              std::any_of(table_it->sequences.begin(), table_it->sequences.end(), [&](const auto& seq) {
                                  return seq.column == table_it->primary_key[0];
                              });
        if (!has_message_id) {
            fail("OUTBOX_PRIMARY_KEY_MUST_BE_AUTOINC_U64",
                 "an outbox needs a uint64_t primary key declared with FIELD_PrimaryKeyAutoInc");
            return;
        }

        if (outbox.target_fields.empty()) {
            fail("OUTBOX_MISSING_TARGET", "declare the target column with FIELD_Target");
            return;
        }
        if (outbox.target_fields.size() > 1) {
            fail("OUTBOX_MULTIPLE_TARGETS", "an outbox has exactly one FIELD_Target column");
            return;
        }
        auto target = column_index(outbox.target_fields[0]);
        if (!target) {
            fail("OUTBOX_TARGET_NOT_FOUND", "target column '" + outbox.target_fields[0] + "' was not found");
            return;
        }
        if (*target == table_it->primary_key[0]) {
            fail("OUTBOX_TARGET_IS_PRIMARY_KEY", "the target column can't be the message ID");
            return;
        }

        // The parameter each column supplies: its alias if it has one, otherwise its own name.
        std::vector<std::string> supplies = outbox.column_names;
        for (size_t i = 0; i < outbox.param_aliases.size(); ++i) {
            const auto& [field, param] = outbox.param_aliases[i];
            for (size_t j = 0; j < i; ++j) {
                if (outbox.param_aliases[j].first == field) {
                    fail("OUTBOX_DUPLICATE_PARAM_ALIAS_" + field, "column '" + field + "' has more than one FIELD_Param");
                    return;
                }
            }
            auto column = column_index(field);
            if (!column) {
                fail("OUTBOX_PARAM_COLUMN_NOT_FOUND_" + field, "FIELD_Param column '" + field + "' was not found");
                return;
            }
            supplies[*column] = param;
        }

        std::vector<uint16_t> arg_columns;
        for (size_t p = 0; p < outbox.param_names.size(); ++p) {
            const std::string& param = outbox.param_names[p];
            std::vector<uint16_t> matches;
            for (uint16_t c = 0; c < supplies.size(); ++c) {
                if (supplies[c] == param) {
                    matches.push_back(c);
                }
            }
            if (matches.empty()) {
                fail("OUTBOX_NO_COLUMN_FOR_PARAM_" + param,
                     "no column matches receiver parameter '" + param +
                         "'; add a column with that name or map one to it with FIELD_Param");
                return;
            }
            if (matches.size() > 1) {
                fail("OUTBOX_MULTIPLE_COLUMNS_FOR_PARAM_" + param,
                     "more than one column matches receiver parameter '" + param + "'");
                return;
            }
            if (outbox.column_types[matches[0]] != outbox.param_types[p]) {
                fail("OUTBOX_PARAM_TYPE_MISMATCH_" + param,
                     "column '" + outbox.column_names[matches[0]] + "' doesn't have the type of receiver parameter '" +
                         param + "'");
                return;
            }
            arg_columns.push_back(matches[0]);
        }

        if (outbox.on_result_reducers.size() > 1) {
            fail("OUTBOX_MULTIPLE_ON_RESULT", "an outbox has at most one on_result reducer");
            return;
        }

        auto signature_hash = DecodeSignatureHash(outbox.signature_hash);
        if (!signature_hash) {
            fail("OUTBOX_INVALID_SIGNATURE_HASH", "the receiver bindings' signature hash is not valid hex");
            return;
        }

        RawOutboxDefV10 def;
        def.table_name = table;
        def.remote_reducer = outbox.remote_reducer;
        def.target_column = *target;
        def.arg_columns = std::move(arg_columns);
        if (!outbox.on_result_reducers.empty()) {
            def.on_result_reducer = outbox.on_result_reducers[0];
        }
        def.signature_hash = *signature_hash;
        outboxes_.push_back(std::move(def));
    }
}

void V10Builder::SetTableIsEventFlag(const std::string& table_name, bool is_event) {
    for (auto& entry : table_is_event_) {
        if (entry.first == table_name) {
            entry.second = is_event;
            for (auto& table : tables_) {
                if (table.source_name == table_name) {
                    table.is_event = is_event;
                    break;
                }
            }
            return;
        }
    }
    table_is_event_.emplace_back(table_name, is_event);
}

bool V10Builder::GetTableIsEventFlag(const std::string& table_name) const {
    for (const auto& entry : table_is_event_) {
        if (entry.first == table_name) {
            return entry.second;
        }
    }
    return false;
}

void V10Builder::RegisterExplicitTableName(const std::string& source_name, const std::string& canonical_name) {
    ExplicitNameEntry entry;
    entry.set<0>(NameMapping{source_name, canonical_name});
    explicit_names_.push_back(std::move(entry));
}

void V10Builder::RegisterExplicitFunctionName(const std::string& source_name, const std::string& canonical_name) {
    ExplicitNameEntry entry;
    entry.set<1>(NameMapping{source_name, canonical_name});
    explicit_names_.push_back(std::move(entry));
}

void V10Builder::RegisterExplicitIndexName(const std::string& source_name, const std::string& canonical_name) {
    ExplicitNameEntry entry;
    entry.set<2>(NameMapping{source_name, canonical_name});
    explicit_names_.push_back(std::move(entry));
}

AlgebraicType V10Builder::MakeUnitAlgebraicType() {
    AlgebraicType unit(AlgebraicType::Tag::Product);
    unit.set<2>(std::make_unique<ProductType>());
    return unit;
}

AlgebraicType V10Builder::MakeStringAlgebraicType() {
    return AlgebraicType(AlgebraicType::Tag::String);
}

AlgebraicType V10Builder::MakeQueryReturnAlgebraicType(AlgebraicType row_type) {
    auto product = std::make_unique<ProductType>();
    product->elements.emplace_back(std::optional<std::string>("__query__"), std::move(row_type));

    AlgebraicType query_type(AlgebraicType::Tag::Product);
    query_type.set<2>(std::move(product));
    return query_type;
}

void V10Builder::UpsertTable(const RawTableDefV10& table) {
    auto it = std::find_if(tables_.begin(), tables_.end(), [&](const auto& existing) {
        return existing.source_name == table.source_name;
    });
    if (it == tables_.end()) {
        tables_.push_back(table);
    } else {
        *it = table;
    }
}

void V10Builder::UpsertLifecycleReducer(const RawLifeCycleReducerDefV10& lifecycle) {
    auto it = std::find_if(lifecycle_reducers_.begin(), lifecycle_reducers_.end(), [&](const auto& existing) {
        return existing.function_name == lifecycle.function_name;
    });
    if (it == lifecycle_reducers_.end()) {
        lifecycle_reducers_.push_back(lifecycle);
    } else {
        *it = lifecycle;
    }
}

void V10Builder::UpsertReducer(const RawReducerDefV10& reducer) {
    auto it = std::find_if(reducers_.begin(), reducers_.end(), [&](const auto& existing) {
        return existing.source_name == reducer.source_name;
    });
    if (it == reducers_.end()) {
        reducers_.push_back(reducer);
    } else {
        *it = reducer;
    }
}

void V10Builder::UpsertProcedure(const RawProcedureDefV10& procedure) {
    auto it = std::find_if(procedures_.begin(), procedures_.end(), [&](const auto& existing) {
        return existing.source_name == procedure.source_name;
    });
    if (it == procedures_.end()) {
        procedures_.push_back(procedure);
    } else {
        *it = procedure;
    }
}

void V10Builder::UpsertView(const RawViewDefV10& view) {
    auto it = std::find_if(views_.begin(), views_.end(), [&](const auto& existing) {
        return existing.source_name == view.source_name;
    });
    if (it == views_.end()) {
        views_.push_back(view);
    } else {
        *it = view;
    }
}

void V10Builder::UpsertHttpHandler(const RawHttpHandlerDefV10& handler) {
    auto it = std::find_if(http_handlers_.begin(), http_handlers_.end(), [&](const auto& existing) {
        return existing.source_name == handler.source_name;
    });
    if (it == http_handlers_.end()) {
        http_handlers_.push_back(handler);
    } else {
        *it = handler;
    }
}

void V10Builder::RegisterHttpHandlerDef(const std::string& handler_name) {
    UpsertHttpHandler(RawHttpHandlerDefV10{handler_name});
}

void V10Builder::RegisterHttpRoute(const RawHttpRouteDefV10& route) {
    http_routes_.push_back(route);
}

void V10Builder::RegisterHttpRouter(const ::SpacetimeDB::Router& router) {
    for (const auto& route : router.routes()) {
        RegisterHttpRoute(RawHttpRouteDefV10{route.handler_name, route.method, route.path});
    }
}

RawIndexDefV10 V10Builder::CreateBTreeIndex(const std::string& table_name,
                                            const std::string& source_name,
                                            const std::vector<uint16_t>& columns,
                                            const std::string& accessor_name) const {
    RawIndexAlgorithmBTreeData btree_data;
    btree_data.columns = columns;
    RawIndexAlgorithm algorithm;
    algorithm.set<0>(btree_data);
    (void)table_name;
    return RawIndexDefV10{
        source_name,
        accessor_name,
        std::move(algorithm),
    };
}

RawConstraintDefV10 V10Builder::CreateUniqueConstraint(const std::string& table_name,
                                                       const std::string& field_name,
                                                       uint16_t field_idx) const {
    RawUniqueConstraintDataV9 unique_data;
    unique_data.columns = {field_idx};
    RawConstraintDataV9 constraint_data;
    constraint_data.set<0>(unique_data);
    (void)table_name;
    (void)field_name;
    return RawConstraintDefV10{
        std::nullopt,
        std::move(constraint_data),
    };
}

RawModuleDefV10 V10Builder::BuildModuleDef() const {
    RawModuleDefV10 v10_module;

    std::vector<RawTypeDefV10> types = types_;

    std::vector<RawReducerDefV10> reducers = reducers_;
    std::vector<RawProcedureDefV10> procedures = procedures_;

    std::unordered_set<std::string> internal_functions;
    for (const auto& lifecycle : lifecycle_reducers_) {
        internal_functions.insert(lifecycle.function_name);
    }
    for (const auto& schedule : schedules_) {
        internal_functions.insert(schedule.function_name);
    }
    for (const auto& outbox : outboxes_) {
        if (outbox.on_result_reducer) {
            internal_functions.insert(*outbox.on_result_reducer);
        }
    }
    for (auto& reducer : reducers) {
        if (internal_functions.find(reducer.source_name) != internal_functions.end()) {
            reducer.visibility = FunctionVisibility::Private;
        }
    }
    for (auto& procedure : procedures) {
        if (internal_functions.find(procedure.source_name) != internal_functions.end()) {
            procedure.visibility = FunctionVisibility::Private;
        }
    }

    RawModuleDefV10Section section_typespace;
    section_typespace.set<0>(typespace_);
    v10_module.sections.push_back(section_typespace);

    if (!types.empty()) {
        RawModuleDefV10Section section_types;
        section_types.set<1>(std::move(types));
        v10_module.sections.push_back(std::move(section_types));
    }
    if (!tables_.empty()) {
        RawModuleDefV10Section section_tables;
        section_tables.set<2>(tables_);
        v10_module.sections.push_back(std::move(section_tables));
    }
    if (!reducers.empty()) {
        RawModuleDefV10Section section_reducers;
        section_reducers.set<3>(std::move(reducers));
        v10_module.sections.push_back(std::move(section_reducers));
    }
    if (!procedures.empty()) {
        RawModuleDefV10Section section_procedures;
        section_procedures.set<4>(std::move(procedures));
        v10_module.sections.push_back(std::move(section_procedures));
    }
    if (!views_.empty()) {
        RawModuleDefV10Section section_views;
        section_views.set<5>(views_);
        v10_module.sections.push_back(std::move(section_views));
    }
    if (!schedules_.empty()) {
        RawModuleDefV10Section section_schedules;
        section_schedules.set<6>(schedules_);
        v10_module.sections.push_back(std::move(section_schedules));
    }
    if (!lifecycle_reducers_.empty()) {
        RawModuleDefV10Section section_lifecycle;
        section_lifecycle.set<7>(lifecycle_reducers_);
        v10_module.sections.push_back(std::move(section_lifecycle));
    }
    if (case_conversion_policy_.has_value()) {
        RawModuleDefV10Section section_case_policy;
        section_case_policy.set<9>(case_conversion_policy_.value());
        v10_module.sections.push_back(std::move(section_case_policy));
    }
    if (!explicit_names_.empty()) {
        RawModuleDefV10Section section_explicit_names;
        section_explicit_names.set<10>(ExplicitNames{explicit_names_});
        v10_module.sections.push_back(std::move(section_explicit_names));
    }
    if (!http_handlers_.empty()) {
        RawModuleDefV10Section section_http_handlers;
        section_http_handlers.set<11>(http_handlers_);
        v10_module.sections.push_back(std::move(section_http_handlers));
    }
    if (!http_routes_.empty()) {
        RawModuleDefV10Section section_http_routes;
        section_http_routes.set<12>(http_routes_);
        v10_module.sections.push_back(std::move(section_http_routes));
    }
    if (!view_primary_keys_.empty()) {
        RawModuleDefV10Section section_view_primary_keys;
        section_view_primary_keys.set<13>(view_primary_keys_);
        v10_module.sections.push_back(std::move(section_view_primary_keys));
    }
    if (!row_level_security_.empty()) {
        RawModuleDefV10Section section_rls;
        section_rls.set<8>(row_level_security_);
        v10_module.sections.push_back(std::move(section_rls));
    }

    validate_environment_declarations();
    RawModuleDefV10Section section_environment;
    section_environment.set<15>(environment_declarations());
    v10_module.sections.push_back(std::move(section_environment));

    // Only modules that declare outboxes get this section, so other modules describe
    // themselves exactly as before.
    if (!outboxes_.empty()) {
        RawModuleDefV10Section section_outboxes;
        section_outboxes.set<16>(outboxes_);
        v10_module.sections.push_back(std::move(section_outboxes));
    }
    return v10_module;
}

} // namespace Internal
} // namespace SpacetimeDB
