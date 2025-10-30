// Note: the generated code depends on APIs and interfaces from crates/bindings-csharp/BSATN.Runtime.
use super::util::fmt_fn;

use std::fmt::{self, Write};
use std::ops::Deref;

use super::code_indenter::CodeIndenter;
use super::Lang;
use crate::util::{
    collect_case, is_reducer_invokable, iter_indexes, iter_reducers, iter_tables, print_auto_generated_file_comment,
    print_auto_generated_version_comment, type_ref_name,
};
use crate::{indent_scope, OutputFile};
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::layout::PrimitiveType;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::def::{BTreeAlgorithm, IndexAlgorithm, ModuleDef, TableDef, TypeDef};
use spacetimedb_schema::schema::{Schema, TableSchema};
use spacetimedb_schema::type_for_generate::{
    AlgebraicTypeDef, AlgebraicTypeUse, PlainEnumTypeDef, ProductTypeDef, SumTypeDef, TypespaceForGenerate,
};

const INDENT: &str = "    ";

const REDUCER_EVENTS: &str = r#"
    public interface IRemoteDbContext : IDbContext<RemoteTables, RemoteReducers, SetReducerFlags, SubscriptionBuilder> {
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError;
    }

    public sealed class EventContext : IEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;

        /// <summary>
        /// The event that caused this callback to run.
        /// </summary>
        public readonly Event<Reducer> Event;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to setters for per-reducer flags.
        ///
        /// The returned <c>SetReducerFlags</c> will have a method to invoke,
        /// for each reducer defined by the module,
        /// which call-flags for the reducer can be set.
        /// </summary>
        public SetReducerFlags SetReducerFlags => conn.SetReducerFlags;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal EventContext(DbConnection conn, Event<Reducer> Event)
        {
            this.conn = conn;
            this.Event = Event;
        }
    }

    public sealed class ReducerEventContext : IReducerEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;
        /// <summary>
        /// The reducer event that caused this callback to run.
        /// </summary>
        public readonly ReducerEvent<Reducer> Event;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to setters for per-reducer flags.
        ///
        /// The returned <c>SetReducerFlags</c> will have a method to invoke,
        /// for each reducer defined by the module,
        /// which call-flags for the reducer can be set.
        /// </summary>
        public SetReducerFlags SetReducerFlags => conn.SetReducerFlags;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal ReducerEventContext(DbConnection conn, ReducerEvent<Reducer> reducerEvent)
        {
            this.conn = conn;
            Event = reducerEvent;
        }
    }

    public sealed class ErrorContext : IErrorContext, IRemoteDbContext
    {
        private readonly DbConnection conn;
        /// <summary>
        /// The <c>Exception</c> that caused this error callback to be run.
        /// </summary>
        public readonly Exception Event;
        Exception IErrorContext.Event {
            get {
                return Event;
            }
        }

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to setters for per-reducer flags.
        ///
        /// The returned <c>SetReducerFlags</c> will have a method to invoke,
        /// for each reducer defined by the module,
        /// which call-flags for the reducer can be set.
        /// </summary>
        public SetReducerFlags SetReducerFlags => conn.SetReducerFlags;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal ErrorContext(DbConnection conn, Exception error)
        {
            this.conn = conn;
            Event = error;
        }
    }

    public sealed class SubscriptionEventContext : ISubscriptionEventContext, IRemoteDbContext
    {
        private readonly DbConnection conn;

        /// <summary>
        /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
        ///
        /// The returned <c>DbView</c> will have a method to access each table defined by the module.
        /// </summary>
        public RemoteTables Db => conn.Db;
        /// <summary>
        /// Access to reducers defined by the module.
        ///
        /// The returned <c>RemoteReducers</c> will have a method to invoke each reducer defined by the module,
        /// plus methods for adding and removing callbacks on each of those reducers.
        /// </summary>
        public RemoteReducers Reducers => conn.Reducers;
        /// <summary>
        /// Access to setters for per-reducer flags.
        ///
        /// The returned <c>SetReducerFlags</c> will have a method to invoke,
        /// for each reducer defined by the module,
        /// which call-flags for the reducer can be set.
        /// </summary>
        public SetReducerFlags SetReducerFlags => conn.SetReducerFlags;
        /// <summary>
        /// Returns <c>true</c> if the connection is active, i.e. has not yet disconnected.
        /// </summary>
        public bool IsActive => conn.IsActive;
        /// <summary>
        /// Close the connection.
        ///
        /// Throws an error if the connection is already closed.
        /// </summary>
        public void Disconnect() {
            conn.Disconnect();
        }
        /// <summary>
        /// Start building a subscription.
        /// </summary>
        /// <returns>A builder-pattern constructor for subscribing to queries,
        /// causing matching rows to be replicated into the client cache.</returns>
        public SubscriptionBuilder SubscriptionBuilder() => conn.SubscriptionBuilder();
        /// <summary>
        /// Get the <c>Identity</c> of this connection.
        ///
        /// This method returns null if the connection was constructed anonymously
        /// and we have not yet received our newly-generated <c>Identity</c> from the host.
        /// </summary>
        public Identity? Identity => conn.Identity;
        /// <summary>
        /// Get this connection's <c>ConnectionId</c>.
        /// </summary>
        public ConnectionId ConnectionId => conn.ConnectionId;
        /// <summary>
        /// Register a callback to be called when a reducer with no handler returns an error.
        /// </summary>
        public event Action<ReducerEventContext, Exception>? OnUnhandledReducerError {
            add => Reducers.InternalOnUnhandledReducerError += value;
            remove => Reducers.InternalOnUnhandledReducerError -= value;
        }

        internal SubscriptionEventContext(DbConnection conn)
        {
            this.conn = conn;
        }
    }

    /// <summary>
    /// Builder-pattern constructor for subscription queries.
    /// </summary>
    public sealed class SubscriptionBuilder
    {
        private readonly IDbConnection conn;

        private event Action<SubscriptionEventContext>? Applied;
        private event Action<ErrorContext, Exception>? Error;

        /// <summary>
        /// Private API, use <c>conn.SubscriptionBuilder()</c> instead.
        /// </summary>
        public SubscriptionBuilder(IDbConnection conn)
        {
            this.conn = conn;
        }

        /// <summary>
        /// Register a callback to run when the subscription is applied.
        /// </summary>
        public SubscriptionBuilder OnApplied(
            Action<SubscriptionEventContext> callback
        )
        {
            Applied += callback;
            return this;
        }

        /// <summary>
        /// Register a callback to run when the subscription fails.
        ///
        /// Note that this callback may run either when attempting to apply the subscription,
        /// in which case <c>Self::on_applied</c> will never run,
        /// or later during the subscription's lifetime if the module's interface changes,
        /// in which case <c>Self::on_applied</c> may have already run.
        /// </summary>
        public SubscriptionBuilder OnError(
            Action<ErrorContext, Exception> callback
        )
        {
            Error += callback;
            return this;
        }

        /// <summary>
        /// Subscribe to the following SQL queries.
        ///
        /// This method returns immediately, with the data not yet added to the DbConnection.
        /// The provided callbacks will be invoked once the data is returned from the remote server.
        /// Data from all the provided queries will be returned at the same time.
        ///
        /// See the SpacetimeDB SQL docs for more information on SQL syntax:
        /// <a href="https://spacetimedb.com/docs/sql">https://spacetimedb.com/docs/sql</a>
        /// </summary>
        public SubscriptionHandle Subscribe(
            string[] querySqls
        ) => new(conn, Applied, Error, querySqls);

        /// <summary>
        /// Subscribe to all rows from all tables.
        ///
        /// This method is intended as a convenience
        /// for applications where client-side memory use and network bandwidth are not concerns.
        /// Applications where these resources are a constraint
        /// should register more precise queries via <c>Self.Subscribe</c>
        /// in order to replicate only the subset of data which the client needs to function.
        ///
        /// This method should not be combined with <c>Self.Subscribe</c> on the same <c>DbConnection</c>.
        /// A connection may either <c>Self.Subscribe</c> to particular queries,
        /// or <c>Self.SubscribeToAllTables</c>, but not both.
        /// Attempting to call <c>Self.Subscribe</c>
        /// on a <c>DbConnection</c> that has previously used <c>Self.SubscribeToAllTables</c>,
        /// or vice versa, may misbehave in any number of ways,
        /// including dropping subscriptions, corrupting the client cache, or panicking.
        /// </summary>
        public void SubscribeToAllTables()
        {
            // Make sure we use the legacy handle constructor here, even though there's only 1 query.
            // We drop the error handler, since it can't be called for legacy subscriptions.
            new SubscriptionHandle(
                conn,
                Applied,
                new string[] { "SELECT * FROM *" }
            );
        }
    }

    public sealed class SubscriptionHandle : SubscriptionHandleBase<SubscriptionEventContext, ErrorContext> {
        /// <summary>
        /// Internal API. Construct <c>SubscriptionHandle</c>s using <c>conn.SubscriptionBuilder</c>.
        /// </summary>
        public SubscriptionHandle(IDbConnection conn, Action<SubscriptionEventContext>? onApplied, string[] querySqls) : base(conn, onApplied, querySqls)
        { }

        /// <summary>
        /// Internal API. Construct <c>SubscriptionHandle</c>s using <c>conn.SubscriptionBuilder</c>.
        /// </summary>
        public SubscriptionHandle(
            IDbConnection conn,
            Action<SubscriptionEventContext>? onApplied,
            Action<ErrorContext, Exception>? onError,
            string[] querySqls
        ) : base(conn, onApplied, onError, querySqls)
        { }
    }
"#;

pub struct Csharp<'opts> {
    pub namespace: &'opts str,
}

impl Lang for Csharp<'_> {
    fn generate_table_file(&self, module: &ModuleDef, table: &TableDef) -> OutputFile {
        let mut output = CsharpAutogen::new(
            self.namespace,
            &[
                "SpacetimeDB.BSATN",
                "SpacetimeDB.ClientApi",
                "System.Collections.Generic",
                "System.Runtime.Serialization",
            ],
            false,
        );

        writeln!(output, "public sealed partial class RemoteTables");
        indented_block(&mut output, |output| {
            let schema = TableSchema::from_module_def(module, table, (), 0.into())
                .validated()
                .expect("Failed to generate table due to validation errors");
            let csharp_table_name = table.name.deref().to_case(Case::Pascal);
            let csharp_table_class_name = csharp_table_name.clone() + "Handle";
            let table_type = type_ref_name(module, table.product_type_ref);

            writeln!(
                output,
                "public sealed class {csharp_table_class_name} : RemoteTableHandle<EventContext, {table_type}>"
            );
            indented_block(output, |output| {
                writeln!(
                    output,
                    "protected override string RemoteTableName => \"{}\";",
                    table.name
                );
                writeln!(output);

                // If this is a table, we want to generate event accessor and indexes
                let product_type = module.typespace_for_generate()[table.product_type_ref]
                    .as_product()
                    .unwrap();

                let mut index_names = Vec::new();

                for idx in iter_indexes(table) {
                    let Some(accessor_name) = idx.accessor_name.as_ref() else {
                        // If there is no accessor name, we shouldn't generate a client-side index accessor.
                        continue;
                    };

                    match &idx.algorithm {
                        IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => {
                            let get_csharp_field_name_and_type = |col_pos: ColId| {
                                let (field_name, field_type) = &product_type.elements[col_pos.idx()];
                                let csharp_field_name_pascal = field_name.deref().to_case(Case::Pascal);
                                let csharp_field_type = ty_fmt(module, field_type);
                                (csharp_field_name_pascal, csharp_field_type)
                            };

                            let (row_to_key, key_type) = match columns.as_singleton() {
                                Some(col_pos) => {
                                    let (field_name, field_type) = get_csharp_field_name_and_type(col_pos);
                                    (format!("row.{field_name}"), field_type.to_string())
                                }
                                None => {
                                    let mut key_accessors = Vec::new();
                                    let mut key_type_elems = Vec::new();
                                    for (field_name, field_type) in columns.iter().map(get_csharp_field_name_and_type) {
                                        key_accessors.push(format!("row.{field_name}"));
                                        key_type_elems.push(format!("{field_type} {field_name}"));
                                    }
                                    (
                                        format!("({})", key_accessors.join(", ")),
                                        format!("({})", key_type_elems.join(", ")),
                                    )
                                }
                            };

                            let csharp_index_name = accessor_name.deref().to_case(Case::Pascal);

                            let mut csharp_index_class_name = csharp_index_name.clone();
                            let csharp_index_base_class_name = if schema.is_unique(columns) {
                                csharp_index_class_name += "UniqueIndex";
                                "UniqueIndexBase"
                            } else {
                                csharp_index_class_name += "Index";
                                "BTreeIndexBase"
                            };

                            writeln!(output, "public sealed class {csharp_index_class_name} : {csharp_index_base_class_name}<{key_type}>");
                            indented_block(output, |output| {
                                writeln!(
                                    output,
                                    "protected override {key_type} GetKey({table_type} row) => {row_to_key};"
                                );
                                writeln!(output);
                                writeln!(output, "public {csharp_index_class_name}({csharp_table_class_name} table) : base(table) {{ }}");
                            });
                            writeln!(output);
                            writeln!(output, "public readonly {csharp_index_class_name} {csharp_index_name};");
                            writeln!(output);

                            index_names.push(csharp_index_name);
                        }
                        _ => todo!(),
                    }
                }

                writeln!(
                    output,
                    "internal {csharp_table_class_name}(DbConnection conn) : base(conn)"
                );
                indented_block(output, |output| {
                    for csharp_index_name in &index_names {
                        writeln!(output, "{csharp_index_name} = new(this);");
                    }
                });

                if let Some(primary_col_index) = schema.pk() {
                    writeln!(output);
                    writeln!(
                        output,
                        "protected override object GetPrimaryKey({table_type} row) => row.{col_name_pascal_case};",
                        col_name_pascal_case = primary_col_index.col_name.deref().to_case(Case::Pascal)
                    );
                }
            });
            writeln!(output);
            writeln!(output, "public readonly {csharp_table_class_name} {csharp_table_name};");
        });

        OutputFile {
            filename: format!("Tables/{}.g.cs", table.name.deref().to_case(Case::Pascal)),
            code: output.into_inner(),
        }
    }

    fn generate_type_files(&self, module: &ModuleDef, typ: &TypeDef) -> Vec<OutputFile> {
        let name = collect_case(Case::Pascal, typ.name.name_segments());
        let filename = format!("Types/{name}.g.cs");
        let code = match &module.typespace_for_generate()[typ.ty] {
            AlgebraicTypeDef::Sum(sum) => autogen_csharp_sum(module, name.clone(), sum, self.namespace),
            AlgebraicTypeDef::Product(prod) => autogen_csharp_tuple(module, name.clone(), prod, self.namespace),
            AlgebraicTypeDef::PlainEnum(plain_enum) => {
                autogen_csharp_plain_enum(name.clone(), plain_enum, self.namespace)
            }
        };

        vec![OutputFile { filename, code }]
    }

    fn generate_reducer_file(&self, module: &ModuleDef, reducer: &spacetimedb_schema::def::ReducerDef) -> OutputFile {
        let mut output = CsharpAutogen::new(
            self.namespace,
            &[
                "SpacetimeDB.ClientApi",
                "System.Collections.Generic",
                "System.Runtime.Serialization",
            ],
            false,
        );

        writeln!(output, "public sealed partial class RemoteReducers : RemoteBase");
        indented_block(&mut output, |output| {
            let func_name_pascal_case = reducer.name.deref().to_case(Case::Pascal);
            let delegate_separator = if reducer.params_for_generate.elements.is_empty() {
                ""
            } else {
                ", "
            };

            let mut func_params: String = String::new();
            let mut func_args: String = String::new();

            for (arg_i, (arg_name, arg_ty)) in reducer.params_for_generate.into_iter().enumerate() {
                if arg_i != 0 {
                    func_params.push_str(", ");
                    func_args.push_str(", ");
                }

                let arg_type_str = ty_fmt(module, arg_ty);
                let arg_name = arg_name.deref().to_case(Case::Camel);

                write!(func_params, "{arg_type_str} {arg_name}").unwrap();
                write!(func_args, "{arg_name}").unwrap();
            }

            writeln!(
                output,
                "public delegate void {func_name_pascal_case}Handler(ReducerEventContext ctx{delegate_separator}{func_params});"
            );
            writeln!(
                output,
                "public event {func_name_pascal_case}Handler? On{func_name_pascal_case};"
            );
            writeln!(output);

            if is_reducer_invokable(reducer) {
                writeln!(output, "public void {func_name_pascal_case}({func_params})");
                indented_block(output, |output| {
                    writeln!(
                        output,
                        "conn.InternalCallReducer(new Reducer.{func_name_pascal_case}({func_args}), this.SetCallReducerFlags.{func_name_pascal_case}Flags);"
                    );
                });
                writeln!(output);
            }

            writeln!(
                output,
                "public bool Invoke{func_name_pascal_case}(ReducerEventContext ctx, Reducer.{func_name_pascal_case} args)"
            );
            indented_block(output, |output| {
                writeln!(output, "if (On{func_name_pascal_case} == null)");
                indented_block(output, |output| {
                    writeln!(output, "if (InternalOnUnhandledReducerError != null)");
                    indented_block(output, |output| {
                        writeln!(output, "switch(ctx.Event.Status)");
                        indented_block(output, |output| {
                            writeln!(output, "case Status.Failed(var reason): InternalOnUnhandledReducerError(ctx, new Exception(reason)); break;");
                            writeln!(output, "case Status.OutOfEnergy(var _): InternalOnUnhandledReducerError(ctx, new Exception(\"out of energy\")); break;");
                        });
                    });
                    writeln!(output, "return false;");
                });

                writeln!(output, "On{func_name_pascal_case}(");
                // Write out arguments one per line
                {
                    indent_scope!(output);
                    write!(output, "ctx");
                    for (arg_name, _) in &reducer.params_for_generate {
                        writeln!(output, ",");
                        let arg_name = arg_name.deref().to_case(Case::Pascal);
                        write!(output, "args.{arg_name}");
                    }
                    writeln!(output);
                }
                writeln!(output, ");");
                writeln!(output, "return true;");
            });
        });

        writeln!(output);

        writeln!(output, "public abstract partial class Reducer");
        indented_block(&mut output, |output| {
            autogen_csharp_product_common(
                module,
                output,
                reducer.name.deref().to_case(Case::Pascal),
                &reducer.params_for_generate,
                "Reducer, IReducerArgs",
                |output| {
                    if !reducer.params_for_generate.elements.is_empty() {
                        writeln!(output);
                    }
                    writeln!(output, "string IReducerArgs.ReducerName => \"{}\";", reducer.name);
                },
            );
        });

        if is_reducer_invokable(reducer) {
            writeln!(output);
            writeln!(output, "public sealed partial class SetReducerFlags");
            indented_block(&mut output, |output| {
                let func_name_pascal_case = reducer.name.deref().to_case(Case::Pascal);
                writeln!(output, "internal CallReducerFlags {func_name_pascal_case}Flags;");
                writeln!(output, "public void {func_name_pascal_case}(CallReducerFlags flags) => {func_name_pascal_case}Flags = flags;");
            });
        }

        OutputFile {
            filename: format!("Reducers/{}.g.cs", reducer.name.deref().to_case(Case::Pascal)),
            code: output.into_inner(),
        }
    }

    fn generate_global_files(&self, module: &ModuleDef) -> Vec<OutputFile> {
        let mut output = CsharpAutogen::new(
            self.namespace,
            &[
                "SpacetimeDB.ClientApi",
                "System.Collections.Generic",
                "System.Runtime.Serialization",
            ],
            true, // print the version in the globals file
        );

        writeln!(output, "public sealed partial class RemoteReducers : RemoteBase");
        indented_block(&mut output, |output| {
            writeln!(
                output,
                "internal RemoteReducers(DbConnection conn, SetReducerFlags flags) : base(conn) => SetCallReducerFlags = flags;"
            );
            writeln!(output, "internal readonly SetReducerFlags SetCallReducerFlags;");
            writeln!(
                output,
                "internal event Action<ReducerEventContext, Exception>? InternalOnUnhandledReducerError;"
            )
        });
        writeln!(output);

        writeln!(output, "public sealed partial class RemoteTables : RemoteTablesBase");
        indented_block(&mut output, |output| {
            writeln!(output, "public RemoteTables(DbConnection conn)");
            indented_block(output, |output| {
                for table in iter_tables(module) {
                    writeln!(
                        output,
                        "AddTable({} = new(conn));",
                        table.name.deref().to_case(Case::Pascal)
                    );
                }
            });
        });
        writeln!(output);

        writeln!(output, "public sealed partial class SetReducerFlags {{ }}");

        writeln!(output, "{REDUCER_EVENTS}");

        writeln!(output, "public abstract partial class Reducer");
        indented_block(&mut output, |output| {
            // Prevent instantiation of this class from outside.
            writeln!(output, "private Reducer() {{ }}");
        });
        writeln!(output);

        writeln!(
            output,
            "public sealed class DbConnection : DbConnectionBase<DbConnection, RemoteTables, Reducer>"
        );
        indented_block(&mut output, |output: &mut CodeIndenter<String>| {
            writeln!(output, "public override RemoteTables Db {{ get; }}");
            writeln!(output, "public readonly RemoteReducers Reducers;");
            writeln!(output, "public readonly SetReducerFlags SetReducerFlags = new();");
            writeln!(output);

            writeln!(output, "public DbConnection()");
            indented_block(output, |output| {
                writeln!(output, "Db = new(this);");
                writeln!(output, "Reducers = new(this, SetReducerFlags);");
            });
            writeln!(output);

            writeln!(output, "protected override Reducer ToReducer(TransactionUpdate update)");
            indented_block(output, |output| {
                writeln!(output, "var encodedArgs = update.ReducerCall.Args;");
                writeln!(output, "return update.ReducerCall.ReducerName switch {{");
                {
                    indent_scope!(output);
                    for reducer in iter_reducers(module) {
                        let reducer_str_name = &reducer.name;
                        let reducer_name = reducer.name.deref().to_case(Case::Pascal);
                        writeln!(
                            output,
                            "\"{reducer_str_name}\" => BSATNHelpers.Decode<Reducer.{reducer_name}>(encodedArgs),"
                        );
                    }
                    writeln!(
                        output,
                        r#""" => throw new SpacetimeDBEmptyReducerNameException("Reducer name is empty"),"#
                    );
                    writeln!(
                        output,
                        r#"var reducer => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {{reducer}}")"#
                    );
                }
                writeln!(output, "}};");
            });
            writeln!(output);

            writeln!(
                output,
                "protected override IEventContext ToEventContext(Event<Reducer> Event) =>"
            );
            writeln!(output, "new EventContext(this, Event);");
            writeln!(output);

            writeln!(
                output,
                "protected override IReducerEventContext ToReducerEventContext(ReducerEvent<Reducer> reducerEvent) =>"
            );
            writeln!(output, "new ReducerEventContext(this, reducerEvent);");
            writeln!(output);

            writeln!(
                output,
                "protected override ISubscriptionEventContext MakeSubscriptionEventContext() =>"
            );
            writeln!(output, "new SubscriptionEventContext(this);");
            writeln!(output);

            writeln!(
                output,
                "protected override IErrorContext ToErrorContext(Exception exception) =>"
            );
            writeln!(output, "new ErrorContext(this, exception);");
            writeln!(output);

            writeln!(
                output,
                "protected override bool Dispatch(IReducerEventContext context, Reducer reducer)"
            );
            indented_block(output, |output| {
                writeln!(output, "var eventContext = (ReducerEventContext)context;");
                writeln!(output, "return reducer switch {{");
                {
                    indent_scope!(output);
                    for reducer_name in iter_reducers(module).map(|r| r.name.deref().to_case(Case::Pascal)) {
                        writeln!(
                            output,
                            "Reducer.{reducer_name} args => Reducers.Invoke{reducer_name}(eventContext, args),"
                        );
                    }
                    writeln!(
                        output,
                        r#"_ => throw new ArgumentOutOfRangeException("Reducer", $"Unknown reducer {{reducer}}")"#
                    );
                }
                writeln!(output, "}};");
            });
            writeln!(output);

            writeln!(output, "public SubscriptionBuilder SubscriptionBuilder() => new(this);");
            writeln!(
                output,
                "public event Action<ReducerEventContext, Exception> OnUnhandledReducerError"
            );
            indented_block(output, |output| {
                writeln!(output, "add => Reducers.InternalOnUnhandledReducerError += value;");
                writeln!(output, "remove => Reducers.InternalOnUnhandledReducerError -= value;");
            });
        });

        vec![OutputFile {
            filename: "SpacetimeDBClient.g.cs".to_owned(),
            code: output.into_inner(),
        }]
    }
}

fn ty_fmt<'a>(module: &'a ModuleDef, ty: &'a AlgebraicTypeUse) -> impl fmt::Display + 'a {
    fmt_fn(move |f| match ty {
        AlgebraicTypeUse::Identity => f.write_str("SpacetimeDB.Identity"),
        AlgebraicTypeUse::ConnectionId => f.write_str("SpacetimeDB.ConnectionId"),
        AlgebraicTypeUse::ScheduleAt => f.write_str("SpacetimeDB.ScheduleAt"),
        AlgebraicTypeUse::Timestamp => f.write_str("SpacetimeDB.Timestamp"),
        AlgebraicTypeUse::TimeDuration => f.write_str("SpacetimeDB.TimeDuration"),
        AlgebraicTypeUse::Unit => f.write_str("SpacetimeDB.Unit"),
        AlgebraicTypeUse::Option(inner_ty) => write!(f, "{}?", ty_fmt(module, inner_ty)),
        AlgebraicTypeUse::Array(elem_ty) => write!(f, "System.Collections.Generic.List<{}>", ty_fmt(module, elem_ty)),
        AlgebraicTypeUse::String => f.write_str("string"),
        AlgebraicTypeUse::Ref(r) => f.write_str(&type_ref_name(module, *r)),
        AlgebraicTypeUse::Primitive(prim) => f.write_str(match prim {
            PrimitiveType::Bool => "bool",
            PrimitiveType::I8 => "sbyte",
            PrimitiveType::U8 => "byte",
            PrimitiveType::I16 => "short",
            PrimitiveType::U16 => "ushort",
            PrimitiveType::I32 => "int",
            PrimitiveType::U32 => "uint",
            PrimitiveType::I64 => "long",
            PrimitiveType::U64 => "ulong",
            PrimitiveType::I128 => "I128",
            PrimitiveType::U128 => "U128",
            PrimitiveType::I256 => "I256",
            PrimitiveType::U256 => "U256",
            PrimitiveType::F32 => "float",
            PrimitiveType::F64 => "double",
        }),
        AlgebraicTypeUse::Never => unimplemented!(),
    })
}

fn default_init(ctx: &TypespaceForGenerate, ty: &AlgebraicTypeUse) -> Option<&'static str> {
    match ty {
        // Options (`T?`) have a default value of null which is fine for us.
        AlgebraicTypeUse::Option(_) => None,
        AlgebraicTypeUse::Ref(r) => match &ctx[*r] {
            // TODO: generate some proper default here (what would it be for tagged enums?).
            AlgebraicTypeDef::Sum(_) => Some("null!"),
            // Simple enums have their own default (variant with value of zero).
            AlgebraicTypeDef::PlainEnum(_) => None,
            AlgebraicTypeDef::Product(_) => Some("new()"),
        },
        // See Sum(_) handling above.
        AlgebraicTypeUse::ScheduleAt => Some("null!"),
        AlgebraicTypeUse::Array(_) => Some("new()"),
        // Strings must have explicit default value of "".
        AlgebraicTypeUse::String => Some(r#""""#),
        // Primitives are initialized to zero automatically.
        AlgebraicTypeUse::Primitive(_) => None,
        // these are structs, they are initialized to zero-filled automatically
        AlgebraicTypeUse::Unit
        | AlgebraicTypeUse::Identity
        | AlgebraicTypeUse::ConnectionId
        | AlgebraicTypeUse::Timestamp
        | AlgebraicTypeUse::TimeDuration => None,
        AlgebraicTypeUse::Never => unimplemented!("never types are not yet supported in C# output"),
    }
}

struct CsharpAutogen {
    output: CodeIndenter<String>,
}

impl Deref for CsharpAutogen {
    type Target = CodeIndenter<String>;

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

impl std::ops::DerefMut for CsharpAutogen {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.output
    }
}

impl CsharpAutogen {
    pub fn new(namespace: &str, extra_usings: &[&str], include_version: bool) -> Self {
        let mut output = CodeIndenter::new(String::new(), INDENT);

        print_auto_generated_file_comment(&mut output);
        if include_version {
            print_auto_generated_version_comment(&mut output);
        }

        writeln!(output, "#nullable enable");
        writeln!(output);

        writeln!(output, "using System;");
        // Don't emit `using SpacetimeDB;` if we are going to be nested in the SpacetimeDB namespace.
        if namespace
            .split('.')
            .next()
            .expect("split always returns at least one string")
            != "SpacetimeDB"
        {
            writeln!(output, "using SpacetimeDB;");
        }
        for extra_using in extra_usings {
            writeln!(output, "using {extra_using};");
        }
        writeln!(output);

        writeln!(output, "namespace {namespace}");
        writeln!(output, "{{");
        output.indent(1);

        Self { output }
    }

    pub fn into_inner(mut self) -> String {
        self.dedent(1);
        writeln!(self, "}}");

        self.output.into_inner()
    }
}

fn autogen_csharp_sum(module: &ModuleDef, sum_type_name: String, sum_type: &SumTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(namespace, &[], false);

    writeln!(output, "[SpacetimeDB.Type]");
    write!(
        output,
        "public partial record {sum_type_name} : SpacetimeDB.TaggedEnum<("
    );
    {
        indent_scope!(output);
        for (i, (variant_name, variant_ty)) in sum_type.variants.iter().enumerate() {
            if i != 0 {
                write!(output, ",");
            }
            writeln!(output);
            write!(output, "{} {variant_name}", ty_fmt(module, variant_ty));
        }
        // If we have fewer than 2 variants, we need to add some dummy variants to make the tuple work.
        match sum_type.variants.len() {
            0 => {
                writeln!(output);
                writeln!(output, "SpacetimeDB.Unit _Reserved1,");
                write!(output, "SpacetimeDB.Unit _Reserved2");
            }
            1 => {
                writeln!(output, ",");
                write!(output, "SpacetimeDB.Unit _Reserved");
            }
            _ => {}
        }
    }
    writeln!(output);
    writeln!(output, ")>;");

    output.into_inner()
}

fn autogen_csharp_plain_enum(enum_type_name: String, enum_type: &PlainEnumTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(namespace, &[], false);

    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "public enum {enum_type_name}");
    indented_block(&mut output, |output| {
        for variant in &*enum_type.variants {
            writeln!(output, "{variant},");
        }
    });

    output.into_inner()
}

fn autogen_csharp_tuple(module: &ModuleDef, name: String, tuple: &ProductTypeDef, namespace: &str) -> String {
    let mut output = CsharpAutogen::new(
        namespace,
        &["System.Collections.Generic", "System.Runtime.Serialization"],
        false,
    );

    autogen_csharp_product_common(module, &mut output, name, tuple, "", |_| {});

    output.into_inner()
}

fn autogen_csharp_product_common(
    module: &ModuleDef,
    output: &mut CodeIndenter<String>,
    name: String,
    product_type: &ProductTypeDef,
    base: &str,
    extra_body: impl FnOnce(&mut CodeIndenter<String>),
) {
    writeln!(output, "[SpacetimeDB.Type]");
    writeln!(output, "[DataContract]");
    write!(output, "public sealed partial class {name}");
    if !base.is_empty() {
        write!(output, " : {base}");
    }
    writeln!(output);
    indented_block(output, |output| {
        let fields = product_type
            .into_iter()
            .map(|(orig_name, ty)| {
                writeln!(output, "[DataMember(Name = \"{orig_name}\")]");

                let field_name = orig_name.deref().to_case(Case::Pascal);
                let ty = ty_fmt(module, ty).to_string();

                writeln!(output, "public {ty} {field_name};");

                (field_name, ty)
            })
            .collect::<Vec<_>>();

        // If we don't have any fields, the default constructor is fine, otherwise we need to generate our own.
        if !fields.is_empty() {
            writeln!(output);

            // Generate fully-parameterized constructor.
            write!(output, "public {name}(");
            if fields.len() > 1 {
                writeln!(output);
            }
            {
                indent_scope!(output);
                for (i, (field_name, ty)) in fields.iter().enumerate() {
                    if i != 0 {
                        writeln!(output, ",");
                    }
                    write!(output, "{ty} {field_name}");
                }
            }
            if fields.len() > 1 {
                writeln!(output);
            }
            writeln!(output, ")");
            indented_block(output, |output| {
                for (field_name, _ty) in fields.iter() {
                    writeln!(output, "this.{field_name} = {field_name};");
                }
            });
            writeln!(output);

            // Generate default constructor.
            writeln!(output, "public {name}()");
            indented_block(output, |output| {
                for ((field_name, _ty), (_field, field_ty)) in fields.iter().zip(product_type) {
                    if let Some(default) = default_init(module.typespace_for_generate(), field_ty) {
                        writeln!(output, "this.{field_name} = {default};");
                    }
                }
            });
        }

        extra_body(output);
    });
}

fn indented_block<R>(output: &mut CodeIndenter<String>, f: impl FnOnce(&mut CodeIndenter<String>) -> R) -> R {
    writeln!(output, "{{");
    let res = f(&mut output.indented(1));
    writeln!(output, "}}");
    res
}
