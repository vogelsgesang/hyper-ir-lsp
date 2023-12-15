# Hyper IR language server

Code intelligence for Hyper IR.

## Features

The plugin focuses on features which make it easier to understand a Hyper IR module:
Syntax highlighting, code navigation, code visualization, code folding, etc.
Below you can find small screen recordings of all those features.

* **Syntax Highlighting**:  As soon as you open a Hyper IR module, you get proper syntax highlighting.
* **Code Folding**: You can fold individual basic blocks or complete functions.
* **Control Flow Visualization**: Use the "Visualize Controlflow" action directly above a function definition to get a rendering of the functions's control flow graph.
* **Code Navigation**:
  * The **Document Outline** shows you a list of all global variables and functions. Double-click on any function to directly jump to it.
  * Use "**go to definition / references**" on function names, variable names and metadata references.
  * **Inlay hints** show the incoming control flow edges for each basic block
* **(Self)-Diagnostics**: The plugin shows syntax errors or other semantic issues. Given that the IR is usually dumped by Hyper, any errors indicate a bug in either Hyper or this extension.

### Non-features

Editing-focused features (e.g., code-completion) are not provided by this plugin.
Those features would provide little value for Hyper IR because no developer ever writes Hyper IR by hand.
Hyper IR is generated by Hyper, and the file format only exists as a human-readable representation of the generated code.
In other words, Hyper IR is written by computers, for human consumption.
Hence, making it simpler for humans to write Hyper IR would serve little purpose.

## Background

As a compiling database engine, Hyper compiles all queries into executable code and then executes that generated code.
As an intermediate step, Hyper generates a "Hyper Intermediate Representation" (short: Hyper IR) module.
This intermediate representation is then lowered into assembly.
Usually, when debugging Hyper's query engine, it is sufficien to debug the Hyper IR module, though, because usually the bug is already in the generated Hyper IR, and not in the translation from Hyper IR to assembly code.
As such, most bugs can be found through close inspection of Hyper IR.

## Installation

For Visual Studio Code, we offer a pre-packaged Visual Studio Code plugin:

1. Download the correct `*.vsix` package for your operating system from the [latest release](https://github.com/vogelsgesang/hyper-ir-lsp/releases/)
2. Inside Visual Studio, press `Cmd` + `Shift` + `P` to open the command picker
3. Choose the "Extension: Install from VSIX..." command
4. In the file picker, choose the downloaded `.vsix` file
5. (Optional) Install the "Graphviz Interactive Preview" extension. This extension will be required to display the control flow graphs of Hyper IR functions.

Usage in other editors (neovim, emacs, IntelliJ, ...) is also possible.
All functionality is contained inside the Language Server. This Language
Server can be reused between editors. The exact way to configure a
language server is different for each editor, read the manual of your
editor to figure out how to configure a language server. You can get
the language server binary by unzipping the `*.vsix` file and extracting
the `hyper-ir-lsp` binary from it.

## Usage

Pass the `dump_ir=1` parameter to `hyperd` to instruct Hyper to dump all generated modules.
Hyper will then create a folder by the name `codegen_<pid>` inside which it dump write all generated Hyper IR modules.
You can then simply open any of the `*.hir` files and this extension will help you navigate and understand it.

## Development

The source code for this extension lives at https://github.com/vogelsgesang/hyper-ir-lsp.
Github Actions automatically build every single commit for all operating systems (Linux, Windows, OSX).
Contributions are welcome. Feel free to just open a pull request.

### Building from source

1. `cd vscode-extension`
2. `pnpm i`
3. `pnpm package`
4. Install the "hyper-ir-lsp-*.vsix" in VS Code

### Backlog

* LSP functionality
    * ✔ Finish tokenizer
    * ✔ Basic parser
    * ✔ Parser: Support for dependency declarations
    * ✔ Parser: Support debug annotation on external functions (forward compatibility)
    * ✔ Parser for function bodies: Assigments & Labels
    * ✔ Parser for function bodies: Branches
    * ✔ Parser for function bodies: phi nodes
    * ✔ Parser for function bodies: switch
    * ✔ Parser for function bodies: overflow arithmetics (`saddbr`, `longmuldivbr`, ...)
    * ✔ Document outline: Variables & Functions
    * ✔ Document outline: Function-local Labels
    * ✔ Go to definition / declaration / references for function
    * ✔ Go to definition / references for debug refs
    * ✔ Go to definition / references for global variables
    * ✔ Go to definition / references for local variables
    * ✔ Go to definition / references for basic blocks
    * ✔ Code folding on function bodies
    * ✔ Code folding on basic blocks
    * ✔ Inlay hint at end of function: Display function name
    * ✔ Inlay hint at basic block: List incoming edges
    * ✔ Report warnings on duplicate function names / variable names
    * ✔ Report warnings on unknown function names / variable names
    * ✔ Report warnings when we failed to extract the basic blocks from a branching instruction
    * ✔ Control flow graph visualization
    * ✔ Add "Go to definition" for proxied functions
    * Hyperlink the stack trace, pointing to the place where a function is defined
    * Hover provider for function-local variables; Show "SSA chain"
    * "Inline variables" debugger support?
    * Incremental sync
    * More robust error recovery in the tokenizer & parser
    * Figure out what those "*.hir.git" files are about which show up in the problems list
    * Support renames (functions, global vars, labels, local vars)
    * Highlight provider for function-local variables (not sure it's worth it? How is this used by VSCode?)
    * Code folding: Use "folded text" as soon as VS Code supports it (https://github.com/microsoft/vscode/pull/170447)
* VS Code extension
    * ✔ Get a packaged VS Code extension
    * ✔ Correct word boundaries
    * ✔ Include README
    * ✔ Write proper README
    * Proper logo
    * Add screenshots / screen recordings to README
    * Use Webassembly instead of native binary
* Configuration for neovim
* Github CI
    * ✔ Compile rust
    * ✔ Run rust test cases
    * ✔ Lint JS
    * ✔ Package VS Code extension
    * ✔ Compile also for Windows and macos
    * ✔ Publish release artifacts to Github Releases
    * Publish to VS Code marketplace
* ✔ Script to auto-generate the Hyper IR dumps using HyperAPI
* Hyper:
    * ✔ Fix printing of references to unnamed globals
    * ✔ Phi node: Missing whitespace around ','
    * ✔ Phi node: Missing ',' between incoming edges
    * ✔ `switch`: Missing ',' between value and default
    * ✔ Conditional break: no ',' between condition and first basicblock
    * ✔ Teardown-functions: move `destructDone` to the end
    * ✔ Missing "pure" modifier
    * ✔ Stop printing stack traces at `main`
    * Improve performance of symbolizing the stack traces
    * Print threadstate and querystate types better
* Code Style / Things I still need to learn about Rust
    * Deduplicate the `just` + `map_with_span` pattern when parsing instructions
    * Tokenizer: only keep "string views"; don't copy out strings
