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

For example:

> To find rows in a table *table* with a given value in a `#[unique]` or `#[primary_key]` column, do:
>
> ```rust
> ctx.db.{table}().{column}().find({value})
> ```
>
> where *column* is the name of the unique column and *value* is the value you're looking for in that column. This is equivalent to:
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

## Reference pages

### Tone, tense and voice

Reference pages should be written in relatively formal language that would seem at home in an encyclopedia or a textbook. Or, say, [the Microsoft .NET API reference](https://learn.microsoft.com/en-us/dotnet/api/?view=net-8.0).

#### Declarative present tense, for behavior of properties, functions and methods

Use the declarative voice when describing how code works or what it does. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> Public static (`Shared` in Visual Basic) members of this type are thread safe. Any instance members are not guaranteed to be thread safe.
>
> An `ArrayList` can support multiple readers concurrently, as long as the collection is not modified. To guarantee the thread safety of the `ArrayList`, all operations must be done through the wrapper returned by the `Synchronized(IList)` method.

#### *Usually* don't refer to the reader

Use second-person pronouns (i.e. "you") sparingly to draw attention to actions the reader should take to work around bugs or avoid footguns. Often these advisories should be pulled out into note, warning or quote-blocks. [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> Enumerating through a collection is intrinsically not a thread-safe procedure. Even when a collection is synchronized, other threads can still modify the collection, which causes the enumerator to throw an exception. To guarantee thread safety during enumeration, you can either lock the collection during the entire enumeration or catch the exceptions resulting from changes made by other threads.

#### *Usually* don't refer to "we" or "us"

Use first-person pronouns sparingly to draw attention to non-technical information like design advice. Always use the first-person plural (i.e. "we" or "us") and never the singular (i.e. "I" or "me"). Often these should be accompanied by marker words like "recommend," "advise," "encourage" or "discourage." [For example](https://learn.microsoft.com/en-us/dotnet/api/system.collections.arraylist?view=net-8.0):

> We don't recommend that you use the `ArrayList` class for new development. Instead, we recommend that you use the generic `List<T>` class.

### Tables and links

Each reference page should have one or more two-column tables, where the left column are namespace-qualified names or signatures, and the right column are one-sentence descriptions. Headers are optional. If the table contains multiple different kinds of items (e.g. types and functions), the left column should include the kind as a suffix. [For example](https://learn.microsoft.com/en-us/dotnet/api/?view=net-8.0):

> | Name | Description |
> |-|-|
> | `Microsoft.CSharp.RuntimeBinder` Namespace | Provides classes and interfaces that support interoperation between Dynamic Language Runtime and C#. |
> | `Microsoft.VisualBasic` Namespace | Contains types that support the Visual Basic Runtime in Visual Basic. |

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

If the described item has any children (e.g. properties and methods of classes, variants of enums), include one or more tables for those children, as described above, followed by subsections for each child item.

### Grammars and syntax

TODO: Figure out the best way to format grammars. What's in the SQL ref looks good. Find out how we made the flow chart graphics, and encourage using those.

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
> #### Can my client connect to multiple modules at the same time?
>
> Yes! Your client can construct as many `DbConnection`s simultaneously as it wants to, each of which will operate independently. If you want to connect to two modules with different schemas, use `spacetime generate` to include bindings for both of them in your client project. Note that SpacetimeDB may reject multiple concurrent connections to the same module by a single client.

## Tutorial pages

TODO: Tutorial pages are more casual. They're basically going to look like [the Rust client quickstart](./sdks/rust/quickstart.md) and [the Rust server quickstart](./modules/rust/quickstart.md), which I still think are pretty good.
