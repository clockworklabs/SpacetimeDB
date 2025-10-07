# SpacetimeDB Documentation Style Guide

## Purpose of this document

This document describes how the documentation in this repo, which winds up on the SpacetimeDB website, should be written. Much of the content in this repository currently does not meet these standards. Reworking everything to meet these standards is a significant undertaking, and in all honesty will probably never be complete, but at the very least we want to avoid generating new text which doesn't meet our standards. We will request changes on or reject docs PRs which do not obey these rules, even if they are updating or replacing existing docs which also did not obey these rules.

## General guidelines

### Target audience

The SpacetimeDB documentation should be digestable and clear for someone who is a competent web or game developer, but does not have a strong grounding in theoretical math or CS. This means we generally want to steer clear of overly terse formal notations, instead using natural language (like, English words) to describe what's going on.

#### The exception: internals docs

We offer some level of leeway on this for documentation of internal, low-level or advanced interfaces. For example, we don't expect the average user to ever need to know the details of the BSATN binary encoding, so we can make some stronger assumptions about the technical background of readers in that context.

On the other hand, this means that docs for these low-level interfaces should be up-front that they're not for everyone. Start each page with something like, "SUBJECT is a low-level implementation detail of HIGHER-LEVEL SYSTEM. Users of HIGHER-LEVEL SYSTEM should not need to worry about SUBJECT. This document is provided for advanced users and those curious about SpacetimeDB internals." Also make the "HIGHER-LEVEL SYSTEM" a link to the documentation for the user-facing component.

### Code formatting

Use triple-backtick code blocks for any example longer than half a line on a 100-character-wide terminal. Always include a relevant language for syntax highlighting; reasonable choices are:

- `csharp`.
- `rust`.
- `typescript`.
- `sql`.

Use single-backtick inline code highlighting for names of variables, functions, methods &c. Where possible, make these links, usually sharpsign anchor links, to the section of documentation which describes that variable.

In normal text, use italics without any backticks for meta-variables which the user is expected to fill in. Always include an anchor, sentence or "where" clause which describes the meaning of the meta-variable. (E.g. is it a table name? A reducer? An arbitrary string the user can choose? The output of some previous command?)

For meta-variables in code blocks, enclose the meta-variable name in `{}` curly braces. Use the same meta-variable names in code as in normal text. Always include a sentence or "where" clause which describes the meaning of the meta-variable.

Do not use single-backtick code highlighting for words which are not variable, function, method or type names. (Or other sorts of defined symbols that appear in actual code.) Similarly, do not use italics for words which are not meta-variables that the reader is expected to substitute. In particular, do not use code highlighting for emphasis or to introduce vocabulary.

Because this meta-syntax is not valid syntax, it should be followed by an example that shows what the result would look like in a
concrete situation.

For example:

> To find rows in a table _table_ with a given value in a `#[unique]` or `#[primary_key]` column, do:
>
> ```rust
> ctx.db.{table}().{column}().find({value})
> ```
>
> where _column_ is the name of the unique column and _value_ is the value you're looking for in that column.
> For example:
>
> ```rust
> ctx.db.people().name().find("Billy")
> ```
>
> This is equivalent to:
>
> ```sql
> SELECT * FROM {table} WHERE {column} = {value}
> ```

### Pseudocode

Avoid writing pseudocode whenever possible; just write actual code in one of our supported languages. If the file you're writing in is relevant to a specific supported language, use that. If the file applies to the system as a whole, write it in as many of our supported languages as you're comfortable, then ping another team member to help with the languages you don't know.

If it's just for instructional purposes, it can be high-level and include calls to made-up functions, so long as those functions have descriptive names. If you do this, include a note before the code block which clarifies that it's not intended to be runnable as-is.

### Describing limitations and future plans

Call missing features "current limitations" and bugs "known issues."

Be up-front about what isn't implemented right now. It's better for our users to be told up front that something is broken or not done yet than for them to expect it to work and to be surprised when it doesn't.

Don't make promises, even weak ones, about what we plan to do in the future, within tutorials or reference documents. Statements about the future belong in a separate "roadmap" or "future plans" document. Our idea of "soon" is often very different from our users', and our priorities shift rapidly and frequently enough that statements about our future plans rarely end up being accurate.

If your document needs to describe a feature that isn't implemented yet, either rewrite to not depend on that feature, or just say that it's a "current limitation" without elaborating further. Include a workaround if there is one.

### Menu items and paths

When describing GUI elements and menu items, like the **Unity Registry** tab, use bolded text to draw attention to any phrases that will appear in the actual UI. Readers will see this bolded text in the documentation and look for it on their screen. Where applicable, include a short description of the type or category of element, like "tab" above, or the **File** menu. This category should not be bolded, since it is not a word the reader can expect to find on their screen.

When describing a chain of accesses through menus and submenus, use the **->** thin arrow (that's `->`, a hyphen followed by a greater-than sign) as a separator, like **File -> Quit** or **Window -> Package Manager**. List the top-level menu first, and proceed left-to-right until you reach the option you want the user to interact with. Include all nested submenus, like **Foo -> Bar -> Baz -> Quux**. Bold the whole sequence, including the arrows.

It's generally not necessary or desirable to tell users where to look for the top-level menu. You may be tempted to write something like, "Open the **File** menu in the upper left, and navigate **File -> Export as -> Export as PDF**." Do not include "in the upper left" unless you are absolutely confident that the menu will be located there on any combination of OS, version, desktop environment, window manager, theming configuration &c. Even within a single system, UI designers are known to move graphical elements around during updates, making statements like "upper left" obsolete and stale. We can generally trust our readers to be familiar with their own systems and the software they use, and none of our documents involve introducing readers to new GUI software. (E.g. the Unity tutorial is targeted at introducing SpacetimeDB to people who already know Unity.) "Open the **File** menu and navigate **File -> Export as -> Export as PDF**" is sufficient.

### Table names

Table names should be in the singular. `user` rather than `users`, `player` rather than `players`, &c. This applies both to SQL code snippets and to modules. In module code, table names should obey the language's casing for method names: in Rust, `snake_case`, and in C#, `PascalCase`. A table which has a row for each player, containing their most recent login time, might be named `player_last_login_time` in a Rust module, or `PlayerLastLoginTime` in a C# module.

## Key vocabulary

There are a small number of key terms that we need to use consistently throughout the documentation.

The most important distinction is the following:

- **Database**: This is the active, running entity that lives on a host. It contains a bunch of tables, like a normal database. It also has extra features: clients can connect to it directly and remotely call its stored procedures.
- **Module**: This is the source code that a developer uses to specify a database. It is a combination of a database schema and a collection of stored procedures. Once built and published, it becomes part of the running database.

A database **has** a module; the module **is part of** the database.

The module does NOT run on a host. The **database** runs on a host.

A client does NOT "connect to the module". A client **connects to the database**.

This distinction is subtle but important. People know what databases are, and we should reinforce that SpacetimeDB is a database. "Module" is a quirky bit of vocabulary we use to refer to collections of stored procedures. A RUNNING APPLICATION IS NOT CALLED A MODULE.

Other key vocabulary:

- (SpacetimeDB) **Host**: the application that hosts **databases**. It is multi-tenant and can host many **databases** at once.
- **Client**: any application that connects to a **database**.
- **End user**: anybody using a **client**.
- **Database developer**: the person who maintains a **database**.
  - DO NOT refer to database developers as "users" in documentation.
    Sometimes we colloquially refer to them as "our users" internally,
    but it is clearer to use the term "database developers" in public.
- **Table**: A set of typed, labeled **rows**. Each row stores data for a number of **columns**. Used to store data in a **database**.
- **Column**: you know what this is.
- **Row**: you know what this is.
  - DO NOT refer to rows as "tuples", because the term overlaps confusingly with "tuple types" in module languages.
    We reserve the word "tuple" to refer to elements of these types.
- **Reducer**: A stored procedure that can be called remotely in order to update a **database**.
  - Confusingly, reducers do not actually "reduce" data in the sense of querying and compressing it to return a result.
    But it is too late to change it. C'est la vie.
- **Connection**: a connection between a **client** and a **database**. Receives an **Address**. A single connection may open multiple **subscriptions**.
- **Subscription**: an active query that mirrors data from the database to a **client**.
- **Address**: identifier for an active connection.
- **Identity**: A combination of an issuing OpenID Connect provider and an Identity Token issued by that provider. Globally unique and public.
  - Technically, "Identity" should be called "Identifier", but it is too late to change it.
  - A particular **end user** may have multiple Identities issued by different providers.
  - Each **database** also has an **Identity**.

## Reference pages

Reference pages are where intermediate users will look to get a view of all of the capabilities of a tool, and where experienced users will check for specific information on behaviors of the types, functions, methods &c they're using. Each user-facing component in the SpacetimeDB ecosystem should have a reference page.

Each reference page should start with an introduction paragraph that says what the component is and when and how the user will interact with it. It should then either include a section describing how to install or set up that component, or a link to another page which accomplishes the same thing.

### Tone, tense and voice

Reference pages should be written in relatively formal language that would seem at home in an encyclopedia or a textbook. Or, say, [the Microsoft .NET API reference](https://learn.microsoft.com/en-us/dotnet/api/?view=net-8.0).

#### Declarative present tense, for behavior of properties, functions and methods

Use the declarative voice when describing how code works or what it does. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> Public static (`Shared` in Visual Basic) members of this type are thread safe. Any instance members are not guaranteed to be thread safe.
>
> An `ArrayList` can support multiple readers concurrently, as long as the collection is not modified. To guarantee the thread safety of the `ArrayList`, all operations must be done through the wrapper returned by the `Synchronized(IList)` method.

#### _Usually_ don't refer to the reader

Use second-person pronouns (i.e. "you") sparingly to draw attention to actions the reader should take to work around bugs or avoid footguns. Often these advisories should be pulled out into note, warning or quote-blocks. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> Enumerating through a collection is intrinsically not a thread-safe procedure. Even when a collection is synchronized, other threads can still modify the collection, which causes the enumerator to throw an exception. To guarantee thread safety during enumeration, you can either lock the collection during the entire enumeration or catch the exceptions resulting from changes made by other threads.

#### _Usually_ don't refer to "we" or "us"

Use first-person pronouns sparingly to draw attention to non-technical information like design advice. Always use the first-person plural (i.e. "we" or "us") and never the singular (i.e. "I" or "me"). Often these should be accompanied by marker words like "recommend," "advise," "encourage" or "discourage." [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> We don't recommend that you use the `ArrayList` class for new development. Instead, we recommend that you use the generic `List<T>` class.

#### _Usually_ Avoid Passive Voice

Use active voice rather than passive voice to avoid ambiguity regarding who is doing the action. Active voice directly attributes actions to the subject, making sentences easier to understand. For example:

- Passive voice: "The method was invoked."
- Active voice: "The user invoked the method."

The second example is more straightforward and clarifies who is performing the action. In most cases, prefer using the active voice to maintain a clear and direct explanation of code behavior.

However, passive voice may be appropriate in certain contexts where the actor is either unknown or irrelevant. In these cases, the emphasis is placed on the action or result rather than the subject performing it. For example:

- "The `Dispose` method is called automatically when the object is garbage collected."

### Tables and links

Each reference page should have one or more two-column tables, where the left column are namespace-qualified names or signatures, and the right column are one-sentence descriptions. Headers are optional. If the table contains multiple different kinds of items (e.g. types and functions), the left column should include the kind as a suffix. [For example](https://learn.microsoft.com/en-us/dotnet/api/?view=net-8.0):

> | Name                                       | Description                                                                                          |
> | ------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
> | `Microsoft.CSharp.RuntimeBinder` Namespace | Provides classes and interfaces that support interoperation between Dynamic Language Runtime and C#. |
> | `Microsoft.VisualBasic` Namespace          | Contains types that support the Visual Basic Runtime in Visual Basic.                                |

The names should be code-formatted, and should be links to a page or section for that definition. The short descriptions should be the same as are used at the start of the linked page or section (see below).

Authors are encouraged to write multiple different tables on the same page, with headers between introducing them. E.g. it may be useful to divide classes from interfaces, or to divide names by conceptual purpose. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections?view=net-8.0):

> # Classes
>
> | ArrayList | Implements the IList interface using an array whose size is dynamically increased as required. |
> | BitArray | Manages a compact array of bit values, which are represented as Booleans, where true indicates that the bit is on (1) and false indicates the bit is off (0). |
>
> ...
>
> # Interfaces
>
> | ICollection | Defines size, enumerators, and synchronization methods for all nongeneric collections. |
> | IComparer | Exposes a method that compares two objects. |
>
> ...

### Sections for individual definitions

#### Header

When writing a section for an individual definition, start with any metadata that users will need to refer to the defined object, like its namespace. Then write a short paragraph, usually just a single sentence, which gives a high-level description of the thing. This sentence should be in the declarative present tense with an active verb. Start with the verb, with the thing being defined as the implied subject. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> ArrayList Class
> [...]
> Namespace: `System.Collections`
> [...]
> Implements the IList interface using an array whose size is dynamically increased as required.

Next, add a triple-backtick code block that contains just the declaration or signature of the variable, function or method you're describing.

What, specifically, counts as the declaration or signature is somewhat context-dependent. A good general rule is that it's everything in the source code to the left of the equals sign `=` or curly braces `{}`. You can edit this to remove implementation details (e.g. superclasses that users aren't supposed to see), or to add information that would be helpful but isn't in the source (e.g. trait bounds on generic parameters of types which aren't required to instantiate the type, but which most methods require, like `Eq + Hash` for `HashMap`). [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> ```csharp
> public class ArrayList : ICloneable, System.Collections.IList
> ```

If necessary, this should be followed by one or more paragraphs of more in-depth description.

#### Examples

Next, within a subheader named "Examples," include a code block with examples.

To the extent possible, this code block should be freestanding. If it depends on external definitions that aren't included in the standard library or are not otherwise automatically accessible, add a note so that users know what they need to supply themselves (e.g. that the `mod module_bindings;` refers to the `quickstart-chat` module). Do not be afraid to paste the same "header" or "prelude" code (e.g. a table declaration) into a whole bunch of code blocks, but try to avoid making easy-to-miss minor edits to such "header" code.

Add comments to this code block which describe what it does. In particular, if the example prints to the console, show the expected output in a comment. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> ```csharp
> using System;
> using System.Collections;
> public class SamplesArrayList  {
>
>    public static void Main()  {
>
>       // Creates and initializes a new ArrayList.
>       ArrayList myAL = new ArrayList();
>       myAL.Add("Hello");
>       myAL.Add("World");
>       myAL.Add("!");
>
>       // Displays the properties and values of the ArrayList.
>       Console.WriteLine( "myAL" );
>       Console.WriteLine( "    Count:    {0}", myAL.Count );
>       Console.WriteLine( "    Capacity: {0}", myAL.Capacity );
>       Console.Write( "    Values:" );
>       PrintValues( myAL );
>    }
>
>    public static void PrintValues( IEnumerable myList )  {
>       foreach ( Object obj in myList )
>          Console.Write( "   {0}", obj );
>       Console.WriteLine();
>    }
> }
>
>
> /*
> This code produces output similar to the following:
>
> myAL
>     Count:    3
>     Capacity: 4
>     Values:   Hello   World   !
>
> */
> ```

#### Child items

If the described item has any children (e.g. properties and methods of classes, variants of enums), include one or more tables for those children, as described above, followed by subsections for each child item. These subsections follow the same format as for the parent items, with a header, declaration, description, examples and tables of any (grand-)children.

If a documentation page ends up with more than 3 layers of nested items, split it so that each top-level item has its own page.

### Grammars and syntax

Reference documents, particularly for SQL or our serialization formats, will sometimes need to specify grammars. Before doing this, be sure you need to, as a grammar specification is scary and confusing to even moderately technical readers. If you're describing data that obeys some other language that readers will be familiar with, write a definition in or suited to that language instead of defining the grammar. For example, when describing a JSON encoding, consider writing a TypeScript-style type instead of a grammar.

If you really do need to describe a grammar, write an EBNF description inside a triple-backticks code block with the `ebnf` language marker. (I assume that any grammar we need to describe will be context-free.) Start with the "topmost" or "entry" nonterminal, i.e. the syntactic construction that we actually want to parse, and work "downward" towards the terminals. For example, when describing SQL, `statement` is at the top, and `literal` and `ident` are at or near the bottom. You don't have to include trivial rules like those for literals.

Then, write a whole bunch of examples under a subheader "Examples" in another tripple-backtick code block, this one with an appropriate language marker for what you're describing. Include at least one simple example and at least one complicated example. Try to include examples which exercise all of the features your grammar can express.

## Overview pages

Landing page type things, usually named `index.md`.

### Tone, tense and voice

Use the same guidelines as for reference pages, except that you can refer to the reader as "you" more often.

### Links

Include as many links to more specific docs pages as possible within the text. Sharp-links to anchors/headers within other docs pages are super valuable here!

### FAQs

If there's any information you want to impart to users but you're not sure how to shoehorn it into any other page or section, just slap it in an "FAQ" section at the bottom of an overview page.

Each FAQ item should start with a subheader, which is phrased as a question a user would ask.

Answer these questions starting with a declarative or conversational sentence. Refer to the asker as "you," and their project as "your client," "your module" or "your app."

For example:

> #### What's the difference between a subscription query and a one-off query?
>
> Subscription queries are incremental: your client receives updates whenever the database state changes, containing only the altered rows. This is an efficient way to maintain a "materialized view," that is, a local copy of some subset of the database. Use subscriptions when you want to watch rows and react to changes, or to keep local copies of rows which you'll read frequently.
>
> A one-off query happens once, and then is done. Use one-off queries to look at rows you only need once.
>
> #### How do I get an authorization token?
>
> You can supply your users with authorization tokens in several different ways; which one is best for you will depend on the needs of your app. [...] (I don't actually want to write a real answer to this question - pgoldman 2024-11-19.)
>
> #### Can my client connect to multiple databases at the same time?
>
> Yes! Your client can construct as many `DbConnection`s simultaneously as it wants to, each of which will operate independently. If you want to connect to two databases with different schemas, use `spacetime generate` to include bindings for both of them in your client project. Note that SpacetimeDB may reject multiple concurrent connections to the same database by a single client.

## Tutorial pages

Tutorials are where we funnel new-to-intermediate users to introduce them to new concepts.

Some tutorials are associated with specific SpacetimeDB components, and should be included in (sub)directories alongside the documentation for those components. Other tutorials are more general or holisitc, touching many different parts of SpacetimeDB to produce a complete game or app, and should stand alone or be grouped into a "tutorials" or "projects" directory.

### Tone, tense and voice

Be friendly, but still precise and professional. Refer to the reader as "you." Make gentle suggestions for optional actions with "can" or "could." When telling them to do something that's required to advance the tutorial, use the imperative voice. When reminding them of past tutorials or preparing them for future ones, say "we," grouping you (the writer) together with the reader. You two are going on a journey together, so get comfortable!

### Scope

You don't have to teach the reader non-SpacetimeDB-specific things. If you're writing a tutorial on Rust modules, for example, assume basic-to-intermediate familiarity with "Rust," so you can focus on teaching the reader about the "modules" part.

### Introduction: tell 'em what you're gonna tell 'em

Each tutorial should start with a statement of its scope (what new concepts are introduced), goal (what you build or do during the tutorial) and prerequisites (what other tutorials you should have finished first).

> In this tutorial, we'll implement a simple chat server as a SpacetimeDB module. We'll learn how to declare tables and to write reducers, functions which run in the database to modify those tables in response to client requests. Before starting, make sure you've [installed SpacetimeDB](https://spacetimedb.com/install) and [logged in with a developer `Identity`](/auth/for-devs).

### Introducing and linking to definitions

The first time a tutorial or series introduces a new type / function / method / &c, include a short paragraph describing what it is and how it's being used in this tutorial. Make sure to link to the reference section on that item.

### Tutorial code

If the tutorial involves writing code, e.g. for a module or client, the tutorial should include the complete result code within its text. Ideally, it should be possible for a reader to copy and paste all the code blocks in the document into a file, effectively concatenating them together, and wind up with a coherent and runnable program. Sometimes this is not possible, e.g. because C# requires wrapping your whole file in a bunch of scopes. In this case, precede each code block with a sentence that describes where the reader is going to paste it.

Include even uninteresting code, like imports! You can rush through these without spending too much time on them, but make sure that every line of code required to make the project work appears in the tutorial.

> spacetime init should have pre-populated server/src/lib.rs with a trivial module. Clear it out so we can write a new, simple module: a bare-bones chat server.
>
> To the top of server/src/lib.rs, add some imports we'll be using:
>
> ```rust
> use spacetimedb::{table, reducer, Table, ReducerContext, Identity, Timestamp};
> ```

For code that _is_ interesting, after the code block, add a description of what the code does. Usually this will be pretty succinct, as the code should hopefully be pretty clear on its own.

### Words for telling the user to write code

When introducing a code block that the user should put in their file, don't say "copy" or "paste." Instead, tell them (in the imperative) to "add" or "write" the code. This emphasizes active participation, as opposed to passive consumption, and implicitly encourages the user to modify the tutorial code if they'd like. Readers who just want to copy and paste will do so without our telling them.

> To `server/src/lib.rs`, add the definition of the connect reducer:
>
> ```rust
> I don't actually need to fill this in.
> ```

### Conclusion

Each tutorial should end with a conclusion section, with a title like "What's next?"

#### Tell 'em what you told 'em

Start the conclusion with a sentence or paragraph that reminds the reader what they accomplished:

> You've just set up your first database in SpacetimeDB, complete with its very own tables and reducers!

#### Tell them what to do next

If this tutorial is part of a series, link to the next entry:

> You can use any of SpacetimDB's supported client languages to do this. Take a look at the quickstart guide for your client language of choice: [Rust](/sdks/rust/quickstart), [C#](/sdks/c-sharp/quickstart), or [TypeScript](/sdks/typescript/quickstart). If you are planning to use SpacetimeDB with the Unity game engine, you can skip right to the [Unity Comprehensive Tutorial](/unity/part-1).

If this tutorial is about a specific component, link to its reference page:

> Check out the [Rust SDK Reference](/sdks/rust) for a more comprehensive view of the SpacetimeDB Rust SDK.

If this tutorial is the end of a series, or ends with a reasonably complete app, throw in some ideas about how the reader could extend it:

> Our basic terminal interface has some limitations. Incoming messages can appear while the user is typing, which is less than ideal. Additionally, the user's input gets mixed with the program's output, making messages the user sends appear twice. You might want to try improving the interface by using [Rustyline](https://crates.io/crates/rustyline), [Cursive](https://crates.io/crates/cursive), or even creating a full-fledged GUI.
>
> Once your chat server runs for a while, you might want to limit the messages your client loads by refining your `Message` subscription query, only subscribing to messages sent within the last half-hour.
>
> You could also add features like:
>
> - Styling messages by interpreting HTML tags and printing appropriate [ANSI escapes](https://en.wikipedia.org/wiki/ANSI_escape_code).
> - Adding a `moderator` flag to the `User` table, allowing moderators to manage users (e.g., time-out, ban).
> - Adding rooms or channels that users can join or leave.
> - Supporting direct messages or displaying user statuses next to their usernames.

#### Complete code

If the tutorial involved writing code, add a link to the complete code. This should be somewhere on GitHub, either as its own repo, or as an example project within an existing repo. Ensure the linked folder has a README.md file which includes:

- The name of the tutorial project.
- How to run or interact with the tutorial project, whatever that means (e.g. publish to maincloud and then `spacetime call`).
- Links to external dependencies (e.g. for client projects, the module which it runs against).
- A back-link to the tutorial that builds this project.

At the end of the tutorial that builds the `quickstart-chat` module in Rust, you might write:

> You can find the full code for this module in [the SpacetimeDB module examples](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/quickstart-chat).
