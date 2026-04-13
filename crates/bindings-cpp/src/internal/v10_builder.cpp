#include "spacetimedb/internal/v10_builder.h"
#include "spacetimedb/internal/autogen/AlgebraicType.g.h"
#include "spacetimedb/internal/autogen/ProductType.g.h"
#include "spacetimedb/internal/autogen/ProductTypeElement.g.h"
#include "spacetimedb/internal/autogen/RawModuleDefV10Section.g.h"
#include "spacetimedb/internal/autogen/RawTypeDefV10.g.h"
#include "spacetimedb/internal/autogen/RawScopedTypeNameV10.g.h"
#include "spacetimedb/internal/autogen/FunctionVisibility.g.h"
#include "spacetimedb/internal/autogen/ExplicitNames.g.h"
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
    schedules_.clear();
    lifecycle_reducers_.clear();
    row_level_security_.clear();
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
    if (!row_level_security_.empty()) {
        RawModuleDefV10Section section_rls;
        section_rls.set<8>(row_level_security_);
        v10_module.sections.push_back(std::move(section_rls));
    }

    return v10_module;
}

} // namespace Internal
} // namespace SpacetimeDB
