#ifndef SPACETIMEDB_V10_BUILDER_H
#define SPACETIMEDB_V10_BUILDER_H

#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <tuple>
#include <type_traits>
#include <unordered_map>
#include <utility>
#include <vector>
#include <cstdio>
#include "../bsatn/bsatn.h"
#include "../database.h"
#include "autogen/CaseConversionPolicy.g.h"
#include "autogen/ExplicitNameEntry.g.h"
#include "autogen/NameMapping.g.h"
#include "autogen/AlgebraicType.g.h"
#include "autogen/SumType.g.h"
#include "autogen/ProductType.g.h"
#include "autogen/ProductTypeElement.g.h"
#include "autogen/RawModuleDefV10.g.h"
#include "autogen/Typespace.g.h"
#include "autogen/RawTableDefV10.g.h"
#include "autogen/RawReducerDefV10.g.h"
#include "autogen/RawProcedureDefV10.g.h"
#include "autogen/RawViewDefV10.g.h"
#include "autogen/RawScheduleDefV10.g.h"
#include "autogen/RawLifeCycleReducerDefV10.g.h"
#include "autogen/RawColumnDefaultValueV10.g.h"
#include "autogen/RawRowLevelSecurityDefV9.g.h"
#include "autogen/RawTypeDefV10.g.h"
#include "field_registration.h"
#include "buffer_pool.h"
#include "runtime_registration.h"
#include "template_utils.h"
#include "module_type_registration.h"

namespace SpacetimeDB {

void fail_reducer(std::string message);

namespace Internal {

class V10Builder {
public:
    V10Builder() = default;

    void Clear();

    template<typename T>
    void RegisterTable(const std::string& table_name, bool is_public, bool is_event = false) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping table registration '%s' because circular reference error is set\n", table_name.c_str());
            return;
        }
        SpacetimeDB::field_registrar<T>::register_fields();
        auto& descriptor_map = SpacetimeDB::get_table_descriptors();
        auto it = descriptor_map.find(&typeid(T));
        if (it == descriptor_map.end()) {
            SetConstraintRegistrationError(
                "TABLE_NO_FIELD_DESCRIPTORS",
                "table='" + table_name + "' has no registered field descriptors");
            return;
        }

        std::vector<bsatn::ProductTypeElement> elements;
        const auto& field_descs = it->second.fields;
        for (const auto& field_desc : field_descs) {
            bsatn::AlgebraicType field_type = field_desc.get_algebraic_type();
            std::string field_type_name = field_desc.get_type_name ? field_desc.get_type_name() : "";
            if (!field_type_name.empty() && field_type.tag() == bsatn::AlgebraicTypeTag::Sum) {
                const auto& sum = field_type.as_sum();
                bool is_option = (sum.variants.size() == 2 && sum.variants[0].name == "some" && sum.variants[1].name == "none");
                bool is_schedule_at = (sum.variants.size() == 2 && sum.variants[0].name == "Interval" && sum.variants[1].name == "Time");
                bool is_result = (sum.variants.size() == 2 && sum.variants[0].name == "ok" && sum.variants[1].name == "err");
                if (!is_option && !is_schedule_at && !is_result) {
                    size_t last_colon = field_type_name.rfind("::");
                    if (last_colon != std::string::npos) {
                        field_type_name = field_type_name.substr(last_colon + 2);
                    }
                    getModuleTypeRegistration().registerTypeByName(field_type_name, field_type, nullptr);
                }
            }
            elements.emplace_back(std::make_optional(field_desc.name), std::move(field_type));
        }

        bsatn::ProductType bsatn_product(std::move(elements));
        bsatn::AlgebraicType table_type = bsatn::AlgebraicType::make_product(
            std::make_unique<bsatn::ProductType>(std::move(bsatn_product)));
        AlgebraicType registered_type = getModuleTypeRegistration().registerType(table_type, "", &typeid(T));
        if (registered_type.get_tag() != AlgebraicType::Tag::Ref) {
            SetConstraintRegistrationError(
                "TABLE_TYPE_NOT_REF",
                "table='" + table_name + "' did not register as a named Ref type");
            return;
        }

        RawTableDefV10 table_def{
            table_name,
            registered_type.get<0>(),
            {},
            {},
            {},
            {},
            TableType::User,
            is_public ? TableAccess::Public : TableAccess::Private,
            column_defaults_by_table_[table_name],
            is_event,
        };
        UpsertTable(table_def);
        SetTableIsEventFlag(table_name, is_event);
    }

    template<typename T>
    void AddFieldConstraint(const std::string& table_name,
                            const std::string& field_name,
                            FieldConstraint constraint) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping field constraint registration '%s.%s' because circular reference error is set\n",
                        table_name.c_str(), field_name.c_str());
            return;
        }
        SpacetimeDB::field_registrar<T>::register_fields();
        auto& descriptor_map = SpacetimeDB::get_table_descriptors();
        auto it = descriptor_map.find(&typeid(T));
        if (it == descriptor_map.end()) {
            SetConstraintRegistrationError(
                "NO_FIELD_DESCRIPTORS",
                "table='" + table_name + "' field='" + field_name + "' has no registered field descriptors");
            return;
        }

        uint16_t field_idx = 0;
        bool field_found = false;
        for (const auto& field_desc : it->second.fields) {
            if (field_desc.name == field_name) {
                field_found = true;
                break;
            }
            field_idx++;
        }
        if (!field_found) {
            SetConstraintRegistrationError(
                "FIELD_NOT_FOUND",
                "table='" + table_name + "' field='" + field_name + "' was not found");
            return;
        }

        auto table_it = FindTable(table_name);
        if (table_it == tables_.end()) {
            SetConstraintRegistrationError(
                "TABLE_NOT_FOUND",
                "table='" + table_name + "' was not registered before applying field constraints");
            return;
        }

        int constraint_bits = static_cast<int>(constraint);
        if (constraint_bits & 0b1000) {
            if (!table_it->primary_key.empty()) {
                SetMultiplePrimaryKeyError(table_name);
                return;
            }
            table_it->primary_key.push_back(field_idx);
            table_it->constraints.push_back(CreateUniqueConstraint(table_name, field_name, field_idx));
            table_it->indexes.push_back(CreateBTreeIndex(table_name, table_name + "_" + field_name + "_idx_btree", {field_idx}, field_name));
        } else if ((constraint_bits & 0b0100) && !(constraint_bits & 0b1000)) {
            table_it->constraints.push_back(CreateUniqueConstraint(table_name, field_name, field_idx));
            table_it->indexes.push_back(CreateBTreeIndex(table_name, table_name + "_" + field_name + "_idx_btree", {field_idx}, field_name));
        } else if ((constraint_bits & 0b0001) && !(constraint_bits & 0b1100)) {
            table_it->indexes.push_back(CreateBTreeIndex(table_name, table_name + "_" + field_name + "_idx_btree", {field_idx}, field_name));
        }

        if (constraint_bits & static_cast<int>(FieldConstraint::AutoInc)) {
            RawSequenceDefV10 seq_def;
            // Defer sequence naming to host-side canonical generation for Rust/C# parity.
            seq_def.source_name = std::nullopt;
            seq_def.column = field_idx;
            seq_def.start = std::nullopt;
            seq_def.increment = SpacetimeDB::I128(1);
            seq_def.min_value = std::nullopt;
            seq_def.max_value = std::nullopt;
            table_it->sequences.push_back(std::move(seq_def));
        }
    }

    template<typename T>
    void AddMultiColumnIndex(const std::string& table_name,
                             const std::string& index_name,
                             const std::vector<std::string>& field_names) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping multi-column index registration '%s.%s' because circular reference error is set\n",
                        table_name.c_str(), index_name.c_str());
            return;
        }
        if (field_names.empty()) {
            SetConstraintRegistrationError(
                "MULTI_INDEX_EMPTY",
                "table='" + table_name + "' index='" + index_name + "' has no fields");
            return;
        }
        SpacetimeDB::field_registrar<T>::register_fields();
        auto& descriptor_map = SpacetimeDB::get_table_descriptors();
        auto it = descriptor_map.find(&typeid(T));
        if (it == descriptor_map.end()) {
            SetConstraintRegistrationError(
                "NO_FIELD_DESCRIPTORS",
                "table='" + table_name + "' index='" + index_name + "' has no registered field descriptors");
            return;
        }

        auto table_it = FindTable(table_name);
        if (table_it == tables_.end()) {
            SetConstraintRegistrationError(
                "TABLE_NOT_FOUND",
                "table='" + table_name + "' index='" + index_name + "' references an unknown table");
            return;
        }

        std::vector<uint16_t> field_indexes;
        for (const std::string& field_name : field_names) {
            uint16_t field_idx = 0;
            bool found = false;
            for (const auto& field_desc : it->second.fields) {
                if (field_desc.name == field_name) {
                    field_indexes.push_back(field_idx);
                    found = true;
                    break;
                }
                field_idx++;
            }
            if (!found) {
                SetConstraintRegistrationError(
                    "FIELD_NOT_FOUND",
                    "table='" + table_name + "' index='" + index_name + "' field='" + field_name + "' was not found");
                return;
            }
        }

        std::string generated_name = table_name + "_" + field_names[0];
        for (size_t i = 1; i < field_names.size(); ++i) {
            generated_name += "_" + field_names[i];
        }
        generated_name += "_idx_btree";
        table_it->indexes.push_back(CreateBTreeIndex(table_name, generated_name, field_indexes, index_name));
    }

    template<typename T>
    void AddColumnDefault(const std::string& table_name,
                          const std::string& field_name,
                          const std::vector<uint8_t>& serialized_value) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping default-value registration '%s.%s' because circular reference error is set\n",
                        table_name.c_str(), field_name.c_str());
            return;
        }
        auto table_it = FindTable(table_name);
        if (table_it == tables_.end()) {
            SetConstraintRegistrationError(
                "TABLE_NOT_FOUND",
                "table='" + table_name + "' default field='" + field_name + "' references an unknown table");
            return;
        }

        SpacetimeDB::field_registrar<T>::register_fields();
        auto& descriptor_map = SpacetimeDB::get_table_descriptors();
        auto it = descriptor_map.find(&typeid(T));
        if (it == descriptor_map.end()) {
            SetConstraintRegistrationError(
                "NO_FIELD_DESCRIPTORS",
                "table='" + table_name + "' default field='" + field_name + "' has no registered field descriptors");
            return;
        }

        bool field_found = false;
        const auto& fields = it->second.fields;
        for (uint16_t i = 0; i < fields.size(); ++i) {
            if (fields[i].name == field_name) {
                field_found = true;
                for (uint16_t pk_col : table_it->primary_key) {
                    if (pk_col == i) {
                        SetConstraintRegistrationError(
                            "DEFAULT_ON_PRIMARY_KEY",
                            "table='" + table_name + "' field='" + field_name + "' cannot have default on primary key");
                        return;
                    }
                }
                for (const auto& constraint : table_it->constraints) {
                    if (constraint.data.get_tag() == 0) {
                        const auto& unique_data = constraint.data.get<0>();
                        if (unique_data.columns.size() == 1 && unique_data.columns[0] == i) {
                            SetConstraintRegistrationError(
                                "DEFAULT_ON_UNIQUE",
                                "table='" + table_name + "' field='" + field_name + "' cannot have default on unique field");
                            return;
                        }
                    }
                }
                for (const auto& sequence : table_it->sequences) {
                    if (sequence.column == i) {
                        SetConstraintRegistrationError(
                            "DEFAULT_ON_AUTOINC",
                            "table='" + table_name + "' field='" + field_name + "' cannot have default on autoincrement field");
                        return;
                    }
                }
                column_defaults_by_table_[table_name].push_back(RawColumnDefaultValueV10{i, serialized_value});
                table_it->default_values = column_defaults_by_table_[table_name];
                break;
            }
        }
        if (!field_found) {
            SetConstraintRegistrationError(
                "FIELD_NOT_FOUND",
                "table='" + table_name + "' default field='" + field_name + "' was not found");
        }
    }

    template<typename Func>
    void RegisterReducer(const std::string& reducer_name,
                         Func func,
                         const std::vector<std::string>& param_names) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping reducer registration '%s' because circular reference error is set\n",
                        reducer_name.c_str());
            return;
        }
        using traits = function_traits<Func>;
        static_assert(traits::arity > 0, "Reducer must have at least one parameter (ReducerContext)");
        if constexpr (traits::arity > 0) {
            using FirstParamType = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<0>>>;
            static_assert(std::is_same_v<FirstParamType, ReducerContext>,
                "First parameter of reducer must be ReducerContext");
        }

        std::function<void(ReducerContext&, BytesSource)> handler;
        if constexpr (traits::arity == 1) {
            handler = [func](ReducerContext& ctx, BytesSource) {
                auto result = func(ctx);
                if (result.is_err()) {
                    ::SpacetimeDB::fail_reducer(result.error());
                }
            };
        } else {
            handler = [func](ReducerContext& ctx, BytesSource args_source) {
                std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
                []<std::size_t... Js>(std::index_sequence<Js...>, Func fn, ReducerContext& ctx_inner, const std::vector<uint8_t>& bytes) {
                    bsatn::Reader reader(bytes.data(), bytes.size());
                    auto args = std::make_tuple(bsatn::deserialize<typename traits::template arg_t<Js + 1>>(reader)...);
                    std::apply([&ctx_inner, fn](auto&&... unpacked) {
                        auto result = fn(ctx_inner, std::forward<decltype(unpacked)>(unpacked)...);
                        if (result.is_err()) {
                            ::SpacetimeDB::fail_reducer(result.error());
                        }
                    }, args);
                }(std::make_index_sequence<traits::arity - 1>{}, func, ctx, args_bytes);
            };
        }
        RegisterReducerHandler(reducer_name, handler, std::nullopt);

        ProductType params;
        if constexpr (traits::arity > 1) {
            auto& type_reg = getModuleTypeRegistration();
            []<std::size_t... Is>(std::index_sequence<Is...>,
                                  ProductType& out_params,
                                  const std::vector<std::string>& names,
                                  ModuleTypeRegistration& reg) {
                (([]<std::size_t I>(ProductType& p,
                                    const std::vector<std::string>& n,
                                    ModuleTypeRegistration& r) {
                    using param_type = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<I + 1>>>;
                    auto bsatn_type = bsatn::bsatn_traits<param_type>::algebraic_type();
                    AlgebraicType internal_type = r.registerType(bsatn_type, "", &typeid(param_type));
                    std::string param_name = (I < n.size()) ? n[I] : ("arg" + std::to_string(I));
                    p.elements.emplace_back(std::make_optional(param_name), std::move(internal_type));
                }.template operator()<Is>(out_params, names, reg)), ...);
            }(std::make_index_sequence<traits::arity - 1>{}, params, param_names, type_reg);
        }

        RawReducerDefV10 reducer_def{
            reducer_name,
            std::move(params),
            FunctionVisibility::ClientCallable,
            MakeUnitAlgebraicType(),
            MakeStringAlgebraicType(),
        };
        UpsertReducer(reducer_def);
    }

    template<typename Func>
    void RegisterLifecycleReducer(const std::string& reducer_name, Func func, Lifecycle lifecycle) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping lifecycle reducer registration '%s' because circular reference error is set\n",
                        reducer_name.c_str());
            return;
        }
        using traits = function_traits<Func>;
        static_assert(traits::arity > 0, "Reducer must have at least one parameter (ReducerContext)");
        if constexpr (traits::arity > 0) {
            using FirstParamType = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<0>>>;
            static_assert(std::is_same_v<FirstParamType, ReducerContext>,
                "First parameter of reducer must be ReducerContext");
        }

        std::function<void(ReducerContext&, BytesSource)> handler;
        if constexpr (traits::arity == 1) {
            handler = [func](ReducerContext& ctx, BytesSource) {
                auto result = func(ctx);
                if (result.is_err()) {
                    ::SpacetimeDB::fail_reducer(result.error());
                }
            };
        } else {
            handler = [func](ReducerContext& ctx, BytesSource args_source) {
                std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
                []<std::size_t... Js>(std::index_sequence<Js...>, Func fn, ReducerContext& ctx_inner, const std::vector<uint8_t>& bytes) {
                    bsatn::Reader reader(bytes.data(), bytes.size());
                    auto args = std::make_tuple(bsatn::deserialize<typename traits::template arg_t<Js + 1>>(reader)...);
                    std::apply([&ctx_inner, fn](auto&&... unpacked) {
                        auto result = fn(ctx_inner, std::forward<decltype(unpacked)>(unpacked)...);
                        if (result.is_err()) {
                            ::SpacetimeDB::fail_reducer(result.error());
                        }
                    }, args);
                }(std::make_index_sequence<traits::arity - 1>{}, func, ctx, args_bytes);
            };
        }
        RegisterReducerHandler(reducer_name, handler, lifecycle);

        RawReducerDefV10 reducer_def{
            reducer_name,
            ProductType{},
            FunctionVisibility::Private,
            MakeUnitAlgebraicType(),
            MakeStringAlgebraicType(),
        };
        UpsertReducer(reducer_def);
        UpsertLifecycleReducer(RawLifeCycleReducerDefV10{lifecycle, reducer_name});
    }

    template<typename Func>
    void RegisterView(const std::string& view_name,
                      Func func,
                      bool is_public,
                      const std::vector<std::string>& param_names = {}) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping view registration '%s' because circular reference error is set\n",
                        view_name.c_str());
            return;
        }
        (void)param_names;
        using traits = function_traits<Func>;
        using ContextType = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<0>>>;
        using ReturnType = typename traits::result_type;
        static_assert(traits::arity > 0, "View must have at least one parameter (ViewContext or AnonymousViewContext)");
        static_assert(std::is_same_v<ContextType, ViewContext> || std::is_same_v<ContextType, AnonymousViewContext>,
            "First parameter of view must be ViewContext or AnonymousViewContext");

        if constexpr (std::is_same_v<ContextType, ViewContext>) {
            std::function<std::vector<uint8_t>(ViewContext&, BytesSource)> handler =
                [func](ViewContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
                    (void)args_source;
                    auto result = func(ctx);
                    auto result_vec = view_result_to_vec(std::move(result));
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result_vec);
                    }
                    return buf.release();
                };
            RegisterViewHandler(view_name, handler);
        } else {
            std::function<std::vector<uint8_t>(AnonymousViewContext&, BytesSource)> handler =
                [func](AnonymousViewContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
                    (void)args_source;
                    auto result = func(ctx);
                    auto result_vec = view_result_to_vec(std::move(result));
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result_vec);
                    }
                    return buf.release();
                };
            RegisterAnonymousViewHandler(view_name, handler);
        }

        auto& type_reg = getModuleTypeRegistration();
        auto bsatn_return = bsatn::bsatn_traits<ReturnType>::algebraic_type();
        AlgebraicType return_type = type_reg.registerType(bsatn_return, "", &typeid(ReturnType));
        bool is_anonymous = std::is_same_v<ContextType, AnonymousViewContext>;
        uint32_t index = static_cast<uint32_t>(is_anonymous ? (GetAnonymousViewHandlerCount() - 1) : (GetViewHandlerCount() - 1));

        RawViewDefV10 view_def{
            view_name,
            index,
            is_public,
            is_anonymous,
            ProductType{},
            return_type,
        };
        UpsertView(view_def);
    }

    template<typename Func>
    void RegisterProcedure(const std::string& procedure_name,
                           Func func,
                           const std::vector<std::string>& param_names = {}) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping procedure registration '%s' because circular reference error is set\n",
                        procedure_name.c_str());
            return;
        }
        using traits = function_traits<Func>;
        using ReturnType = typename traits::result_type;
        static_assert(traits::arity > 0, "Procedure must have at least one parameter (ProcedureContext)");
        if constexpr (traits::arity > 0) {
            using FirstParamType = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<0>>>;
            static_assert(std::is_same_v<FirstParamType, ProcedureContext>,
                "First parameter of procedure must be ProcedureContext");
        }

        std::function<std::vector<uint8_t>(ProcedureContext&, BytesSource)> handler;
        if constexpr (traits::arity == 1) {
            handler = [func](ProcedureContext& ctx, BytesSource) -> std::vector<uint8_t> {
                auto result = func(ctx);
                IterBuf buf = IterBuf::take();
                {
                    bsatn::Writer writer(buf.get());
                    bsatn::serialize(writer, result);
                }
                return buf.release();
            };
        } else {
            handler = [func](ProcedureContext& ctx, BytesSource args_source) -> std::vector<uint8_t> {
                std::vector<uint8_t> args_bytes = ConsumeBytes(args_source);
                return []<std::size_t... Js>(std::index_sequence<Js...>, Func fn, ProcedureContext& ctx_inner, const std::vector<uint8_t>& bytes) -> std::vector<uint8_t> {
                    bsatn::Reader reader(bytes.data(), bytes.size());
                    auto args = std::make_tuple(bsatn::deserialize<typename traits::template arg_t<Js + 1>>(reader)...);
                    auto result = std::apply([&ctx_inner, fn](auto&&... unpacked) {
                        return fn(ctx_inner, std::forward<decltype(unpacked)>(unpacked)...);
                    }, args);
                    IterBuf buf = IterBuf::take();
                    {
                        bsatn::Writer writer(buf.get());
                        bsatn::serialize(writer, result);
                    }
                    return buf.release();
                }(std::make_index_sequence<traits::arity - 1>{}, func, ctx, args_bytes);
            };
        }
        RegisterProcedureHandler(procedure_name, handler);

        auto& type_reg = getModuleTypeRegistration();
        auto bsatn_return = bsatn::bsatn_traits<ReturnType>::algebraic_type();
        AlgebraicType return_type = type_reg.registerType(bsatn_return, "", &typeid(ReturnType));

        ProductType params;
        if constexpr (traits::arity > 1) {
            []<std::size_t... Is>(std::index_sequence<Is...>,
                                  ProductType& out_params,
                                  const std::vector<std::string>& names,
                                  ModuleTypeRegistration& reg) {
                (([]<std::size_t I>(ProductType& p,
                                    const std::vector<std::string>& n,
                                    ModuleTypeRegistration& r) {
                    using param_type = std::remove_cv_t<std::remove_reference_t<typename traits::template arg_t<I + 1>>>;
                    auto bsatn_type = bsatn::bsatn_traits<param_type>::algebraic_type();
                    AlgebraicType internal_type = r.registerType(bsatn_type, "", &typeid(param_type));
                    std::string param_name = (I < n.size()) ? n[I] : ("arg" + std::to_string(I));
                    p.elements.emplace_back(std::make_optional(param_name), std::move(internal_type));
                }.template operator()<Is>(out_params, names, reg)), ...);
            }(std::make_index_sequence<traits::arity - 1>{}, params, param_names, type_reg);
        }

        RawProcedureDefV10 procedure_def{
            procedure_name,
            std::move(params),
            return_type,
            FunctionVisibility::ClientCallable,
        };
        UpsertProcedure(procedure_def);
    }

    void RegisterSchedule(const std::string& table_name, uint16_t scheduled_at_column, const std::string& reducer_name) {
        if (g_circular_ref_error) {
            std::fprintf(stderr, "ERROR: Skipping schedule registration for table '%s' because circular reference error is set\n",
                        table_name.c_str());
            return;
        }
        std::optional<std::string> schedule_name = table_name + "_sched";
        auto it = std::find_if(schedules_.begin(), schedules_.end(), [&](const auto& schedule) {
            return schedule.table_name == table_name;
        });
        RawScheduleDefV10 schedule{schedule_name, table_name, scheduled_at_column, reducer_name};
        if (it == schedules_.end()) {
            schedules_.push_back(std::move(schedule));
        } else {
            *it = std::move(schedule);
        }
    }

    void RegisterRowLevelSecurity(const std::string& sql_query) {
        row_level_security_.push_back(RawRowLevelSecurityDefV9{sql_query});
    }

    void SetTableIsEventFlag(const std::string& table_name, bool is_event);
    bool GetTableIsEventFlag(const std::string& table_name) const;

    void SetCaseConversionPolicy(CaseConversionPolicy policy) {
        case_conversion_policy_ = policy;
    }

    void RegisterExplicitTableName(const std::string& source_name, const std::string& canonical_name);
    void RegisterExplicitFunctionName(const std::string& source_name, const std::string& canonical_name);
    void RegisterExplicitIndexName(const std::string& source_name, const std::string& canonical_name);

    RawModuleDefV10 BuildModuleDef() const;
    Typespace& GetTypespace() { return typespace_; }
    const Typespace& GetTypespace() const { return typespace_; }
    std::vector<RawTypeDefV10>& GetTypeDefs() { return types_; }
    const std::vector<RawTypeDefV10>& GetTypeDefs() const { return types_; }
    std::vector<RawTableDefV10>& GetTables() { return tables_; }
    const std::vector<RawTableDefV10>& GetTables() const { return tables_; }
    std::vector<RawReducerDefV10>& GetReducers() { return reducers_; }
    const std::vector<RawReducerDefV10>& GetReducers() const { return reducers_; }
    const std::optional<CaseConversionPolicy>& GetCaseConversionPolicy() const { return case_conversion_policy_; }
    const std::vector<ExplicitNameEntry>& GetExplicitNames() const { return explicit_names_; }

private:
    std::vector<RawTableDefV10>::iterator FindTable(const std::string& table_name) {
        return std::find_if(tables_.begin(), tables_.end(), [&](const auto& table) { return table.source_name == table_name; });
    }
    void UpsertTable(const RawTableDefV10& table);
    void UpsertLifecycleReducer(const RawLifeCycleReducerDefV10& lifecycle);
    void UpsertReducer(const RawReducerDefV10& reducer);
    void UpsertProcedure(const RawProcedureDefV10& procedure);
    void UpsertView(const RawViewDefV10& view);
    RawIndexDefV10 CreateBTreeIndex(const std::string& table_name,
                                    const std::string& source_name,
                                    const std::vector<uint16_t>& columns,
                                    const std::string& accessor_name) const;
    RawConstraintDefV10 CreateUniqueConstraint(const std::string& table_name,
                                               const std::string& field_name,
                                               uint16_t field_idx) const;
    static AlgebraicType MakeUnitAlgebraicType();
    static AlgebraicType MakeStringAlgebraicType();

    std::vector<std::pair<std::string, bool>> table_is_event_;
    std::optional<CaseConversionPolicy> case_conversion_policy_;
    std::vector<ExplicitNameEntry> explicit_names_;
    std::unordered_map<std::string, std::vector<RawColumnDefaultValueV10>> column_defaults_by_table_;
    std::vector<RawTableDefV10> tables_;
    std::vector<RawReducerDefV10> reducers_;
    std::vector<RawProcedureDefV10> procedures_;
    std::vector<RawViewDefV10> views_;
    std::vector<RawScheduleDefV10> schedules_;
    std::vector<RawLifeCycleReducerDefV10> lifecycle_reducers_;
    std::vector<RawRowLevelSecurityDefV9> row_level_security_;
    Typespace typespace_{};
    std::vector<RawTypeDefV10> types_;
};

extern std::unique_ptr<V10Builder> g_v10_builder;

void initializeV10Builder();
V10Builder& getV10Builder();

} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_V10_BUILDER_H

