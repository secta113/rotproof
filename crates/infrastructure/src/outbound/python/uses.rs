//! What a Python file uses of what is not its own code, with Python's own static scoping: the one reader of names that
//! the checks of Python share (`domain::code::Use`).
//!
//! - **Scopes** are the module, a function (and a lambda), a class body, which does not enclose the functions and
//!   comprehensions inside it, and a comprehension. A `def` or `class` with type parameters (`def f[T]()`) and a `type`
//!   statement open a scope of their own for them, between the one they sit in and their body.
//! - **A name is bound** by an assignment (augmented, annotated, and the walrus `:=`, which binds in the nearest scope
//!   that is not a comprehension), a parameter, `def`, `class`, `import` (also inside a function), `for`, `with`,
//!   `except ... as`, a capture of `match`, `del` and a type parameter. It is the name of the scope it is bound in at
//!   all, as Python's compiler decides: a name assigned anywhere in a function is local in all of it, and an
//!   `import` below its use is still the name of the use. `global` binds in, and reads from, the module; `nonlocal`
//!   reads from the nearest enclosing function that binds the name, and what it binds there is not followed.
//! - **A name resolves** from the scope of its use outwards, class bodies skipped but the one the use is in, to the
//!   first scope that binds it. When that scope binds it by `import` (any of its bindings), the name is the module or
//!   the name the import takes: `import a.b as c` binds `c` to `a.b`, `import a.b` binds `a` to `a`, `from a import b
//!   as c` binds `c` to `a.b`, a relative import is resolved against the file's package. When it binds it by anything
//!   else the name is the file's own and is no use, and when two bindings of a scope import two different things
//!   (`try: import a as m` and `except: import b as m`), the use is of each. A name no scope binds is `builtins.<name>`
//!   when it is a builtin ([`domain::python_builtins`]) and nothing otherwise.
//! - **A use** is the longest chain of names a use starts: `os.path.join` (attributes of a name, not of a call or a
//!   subscript). It is called when the chain is the function of a call, with the number of arguments written, and a
//!   value otherwise. A name, an attribute or a subscript in an annotation is in a type position; a call in it, and
//!   all it holds, is not (`Annotated[int, Field(default_factory=datetime.now)]`), as it runs.
//! - **A star import** (`from os import *`) is recorded as one, with the module: its names cannot be read, so a name
//!   the file then uses and no scope binds is no use (but a builtin is, as ever).
//!
//! The reader gives the calls that import a module (`importlib.import_module("m")`, `__import__("m")`) as imports of
//! the module, and the ones whose module it cannot read as such, with `eval` and `exec`, called or taken as a value.
//!
//! It gives every call of an attribute, whatever it is the attribute of (`p.read_text()`, `Path("a").exists()`,
//! `self.repo.mkdir(name)`), with the name, how many arguments, what the first is (`None`, a string literal) and what
//! the callee resolves to when its chain starts at an import or a builtin: a rule that judges a method by its name
//! alone, since the type of its receiver cannot be read, asks for them. And it gives every method a class defines,
//! with how many parameters it takes without `self`, for what that rule leaves out.
//!
//! What it does not resolve: a name through an assignment (`o = os`, then `o.environ`), through `globals()`,
//! `getattr` or `exec`, an attribute of an object (`Path("~").expanduser()`, `self.clock.now()`), a string annotation,
//! a name a star import brings in, and the name a class body takes from the module before it binds it (`os = os` in a
//! class body: the name is the class's, as it is for the whole of the body).

use std::collections::{BTreeMap, BTreeSet};
use std::mem::replace;

use ruff_python_ast::visitor::{Visitor, walk_pattern, walk_stmt};
use ruff_python_ast::{
    Arguments, Comprehension, ExceptHandler, Expr, ExprContext, Parameters, Pattern, Stmt,
    StmtClassDef, StmtFunctionDef, TypeParam, TypeParams,
};

use domain::code::{Defined, First, How, Method, Use};
use domain::python_builtins::is_builtin;
use domain::text::Lines;

use super::base_of;

/// The functions a call imports a module with, by the name it resolves to.
const IMPORTS: [&str; 2] = ["importlib.import_module", "builtins.__import__"];

/// The builtins that run code a reader cannot read: called or taken as a value, they are unread.
const RUNS_CODE: [&str; 2] = ["builtins.eval", "builtins.exec"];

/// What the reader gives of one file.
pub struct Found {
    /// Every use, in the order of the lines
    pub uses: Vec<Use>,
    /// The calls that import a module by a string literal: the line of the arguments, and the module's parts
    pub dynamic: Vec<(usize, Vec<String>)>,
    /// The calls that import a module by a name that is no such literal (the line, and the function called), and
    /// every `eval` and `exec`
    pub unread: Vec<(usize, String)>,
    /// Every call of an attribute, in the order of the lines
    pub methods: Vec<Method>,
    /// Every method a class defines
    pub defined: Vec<Defined>,
}

/// Read a file's body, whose relative imports start from `package`.
pub fn read(source: &str, body: &[Stmt], package: &[String]) -> Found {
    let mut reader = Reader {
        lines: Lines::new(source),
        package,
        scopes: vec![Scope::new(Kind::Module, None)],
        current: 0,
        in_type: false,
        entries: Vec::new(),
        methods: Vec::new(),
        defined: Vec::new(),
    };
    reader.visit_body(body);
    reader.finish()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Module,
    Function,
    Class,
    Comprehension,
    TypeParams,
}

/// What binds a name in a scope: an import of what is named, or anything else.
#[derive(Clone, PartialEq, Eq)]
enum Binding {
    Local,
    Import(String),
}

struct Scope {
    kind: Kind,
    parent: Option<usize>,
    bindings: BTreeMap<String, Vec<Binding>>,
    /// The names declared `global` or `nonlocal` in it
    global: BTreeSet<String>,
    nonlocal: BTreeSet<String>,
}

impl Scope {
    fn new(kind: Kind, parent: Option<usize>) -> Self {
        Scope {
            kind,
            parent,
            bindings: BTreeMap::new(),
            global: BTreeSet::new(),
            nonlocal: BTreeSet::new(),
        }
    }
}

/// The first argument of a call, which is the module of an import by a call.
enum Argument {
    Absent,
    Literal(String),
    Other,
}

/// What a call says of itself.
struct Call {
    positional: usize,
    keywords: Vec<String>,
    unpacked: bool,
    first: Argument,
    /// What the first positional argument is, for the rules that ask whether it is `None` or a string
    lead: First,
    /// The line of its arguments, which is where an import by a call is said to be
    arguments_line: usize,
}

/// A use whose name is not yet resolved: scopes are known once the file is read.
struct Pending {
    scope: usize,
    chain: Vec<String>,
    line: usize,
    call: Option<Call>,
    in_type: bool,
}

enum Entry {
    Resolved(Use),
    Pending(Pending),
}

/// A call of an attribute whose callee is not yet resolved.
struct PendingMethod {
    scope: usize,
    /// The chain of names the callee is, when it is one that starts at a name
    chain: Option<Vec<String>>,
    name: String,
    line: usize,
    call: Call,
}

/// What a name resolves to, from a scope.
enum Resolved {
    Imports(Vec<String>),
    /// Bound as the file's own: no use, and no builtin
    Own,
    Unbound,
}

struct Reader<'s> {
    lines: Lines,
    package: &'s [String],
    scopes: Vec<Scope>,
    current: usize,
    in_type: bool,
    entries: Vec<Entry>,
    methods: Vec<PendingMethod>,
    defined: Vec<Defined>,
}

impl Reader<'_> {
    fn line(&self, at: usize) -> usize {
        self.lines.line(at)
    }

    /// Open a scope inside the current one, and make it the current.
    fn push(&mut self, kind: Kind) {
        self.scopes.push(Scope::new(kind, Some(self.current)));
        self.current = self.scopes.len() - 1;
    }

    /// Bind `name` in the current scope, or in the module when the scope declares it `global`; a name it declares
    /// `nonlocal` is bound in an enclosing function, which is not followed.
    fn bind(&mut self, name: &str, binding: Binding) {
        self.bind_in(self.current, name, binding);
    }

    fn bind_in(&mut self, scope: usize, name: &str, binding: Binding) {
        let scope = if self.scopes[scope].global.contains(name) {
            0
        } else if self.scopes[scope].nonlocal.contains(name) {
            return;
        } else {
            scope
        };
        let bound = self.scopes[scope]
            .bindings
            .entry(name.to_string())
            .or_default();
        if !bound.contains(&binding) {
            bound.push(binding);
        }
    }

    /// The scope a walrus binds in: the nearest that is no comprehension.
    fn walrus_scope(&self) -> usize {
        let mut at = self.current;
        while self.scopes[at].kind == Kind::Comprehension {
            at = self.scopes[at].parent.unwrap_or(0);
        }
        at
    }

    /// Record a use of the chain of names that starts at the byte `at`.
    fn record(&mut self, chain: Vec<String>, at: usize, call: Option<Call>) {
        let line = self.line(at);
        self.entries.push(Entry::Pending(Pending {
            scope: self.current,
            chain,
            line,
            call,
            in_type: self.in_type,
        }));
    }

    /// What a call says of itself.
    fn call(&self, arguments: &Arguments) -> Call {
        let starred = arguments.args.iter().any(Expr::is_starred_expr);
        let kwargs = arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none());
        let first = match arguments.find_argument_value("name", 0) {
            Some(Expr::StringLiteral(module)) => {
                Argument::Literal(module.value.to_str().to_string())
            }
            Some(_) => Argument::Other,
            None if starred || kwargs => Argument::Other,
            None => Argument::Absent,
        };
        let lead = match arguments.args.first() {
            Some(Expr::NoneLiteral(_)) => First::Null,
            Some(Expr::StringLiteral(_)) => First::Text,
            _ => First::Other,
        };
        Call {
            lead,
            positional: arguments
                .args
                .iter()
                .filter(|arg| !arg.is_starred_expr())
                .count(),
            keywords: arguments
                .keywords
                .iter()
                .filter_map(|keyword| keyword.arg.as_ref().map(|name| name.to_string()))
                .collect(),
            unpacked: starred || kwargs,
            first,
            arguments_line: self.line(arguments.range.start().to_usize()),
        }
    }

    /// Open the scope of the type parameters of a `def`, a `class` or a `type` statement, if it has any, and bind them.
    fn type_params(&mut self, params: Option<&TypeParams>) {
        let Some(params) = params else { return };
        self.push(Kind::TypeParams);
        for param in &params.type_params {
            let name = match param {
                TypeParam::TypeVar(param) => &param.name,
                TypeParam::TypeVarTuple(param) => &param.name,
                TypeParam::ParamSpec(param) => &param.name,
            };
            self.bind(name.as_str(), Binding::Local);
        }
        self.visit_type_params(params);
    }

    /// Bind the names of the parameters in the current scope.
    fn bind_parameters(&mut self, parameters: &Parameters) {
        for parameter in parameters.iter() {
            self.bind(parameter.as_parameter().name.as_str(), Binding::Local);
        }
    }

    /// The defaults of the parameters, evaluated where the function is defined.
    fn defaults<'a>(&mut self, parameters: &'a Parameters)
    where
        Self: Visitor<'a>,
    {
        for parameter in parameters.iter_non_variadic_params() {
            if let Some(default) = parameter.default.as_deref() {
                self.visit_expr(default);
            }
        }
    }

    fn function(&mut self, def: &StmtFunctionDef) {
        if self.scopes[self.current].kind == Kind::Class {
            let is_static = def.decorator_list.iter().any(|decorator| {
                matches!(&decorator.expression, Expr::Name(name) if name.id.as_str() == "staticmethod")
            });
            let all = def.parameters.iter().count();
            self.defined.push(Defined {
                name: def.name.to_string(),
                parameters: if is_static {
                    all
                } else {
                    all.saturating_sub(1)
                },
            });
        }
        self.bind(def.name.as_str(), Binding::Local);
        for decorator in &def.decorator_list {
            self.visit_decorator(decorator);
        }
        self.defaults(&def.parameters);
        let outer = self.current;
        self.type_params(def.type_params.as_deref());
        for parameter in def.parameters.iter() {
            if let Some(annotation) = parameter.as_parameter().annotation() {
                self.visit_annotation(annotation);
            }
        }
        if let Some(returns) = &def.returns {
            self.visit_annotation(returns);
        }
        self.push(Kind::Function);
        self.bind_parameters(&def.parameters);
        self.visit_body(&def.body);
        self.current = outer;
    }

    fn class(&mut self, class: &StmtClassDef) {
        self.bind(class.name.as_str(), Binding::Local);
        for decorator in &class.decorator_list {
            self.visit_decorator(decorator);
        }
        let outer = self.current;
        self.type_params(class.type_params.as_deref());
        if let Some(arguments) = &class.arguments {
            self.visit_arguments(arguments);
        }
        self.push(Kind::Class);
        self.visit_body(&class.body);
        self.current = outer;
    }

    /// A comprehension: the first iterable is evaluated where the comprehension is, the rest in its own scope.
    fn comprehension<'a>(
        &mut self,
        generators: &'a [Comprehension],
        element: impl FnOnce(&mut Self),
    ) where
        Self: Visitor<'a>,
    {
        let outer = self.current;
        if let Some(first) = generators.first() {
            self.visit_expr(&first.iter);
        }
        self.push(Kind::Comprehension);
        for (at, generator) in generators.iter().enumerate() {
            if at > 0 {
                self.visit_expr(&generator.iter);
            }
            self.visit_expr(&generator.target);
            for condition in &generator.ifs {
                self.visit_expr(condition);
            }
        }
        element(self);
        self.current = outer;
    }

    /// Take an `import`: what each name binds, and, for `from`, the names taken, as uses.
    fn import(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Import(import) => {
                for alias in &import.names {
                    let name = alias.name.as_str();
                    match &alias.asname {
                        Some(local) => self.bind(local.as_str(), Binding::Import(name.to_string())),
                        None => {
                            let top = name.split('.').next().unwrap_or(name);
                            self.bind(top, Binding::Import(top.to_string()));
                        }
                    }
                }
            }
            Stmt::ImportFrom(import) => {
                let base = base_of(self.package, import.level, import.module.as_deref());
                for alias in &import.names {
                    let line = self.line(alias.range.start().to_usize());
                    let Some(base) = &base else {
                        // Above the top package: Python fails on it, so what it names is nothing of the file's
                        if alias.name.as_str() != "*" {
                            let local = alias.asname.as_ref().unwrap_or(&alias.name);
                            self.bind(local.as_str(), Binding::Local);
                        }
                        continue;
                    };
                    if alias.name.as_str() == "*" {
                        self.entries.push(Entry::Resolved(Use {
                            line,
                            name: base.join("."),
                            how: How::Star,
                            in_type: false,
                        }));
                        continue;
                    }
                    let target = base
                        .iter()
                        .map(String::as_str)
                        .chain([alias.name.as_str()])
                        .collect::<Vec<_>>()
                        .join(".");
                    let local = alias.asname.as_ref().unwrap_or(&alias.name);
                    self.bind(local.as_str(), Binding::Import(target.clone()));
                    self.entries.push(Entry::Resolved(Use {
                        line,
                        name: target,
                        how: How::Imported,
                        in_type: false,
                    }));
                }
            }
            _ => {}
        }
    }

    /// What `name` is, used from `scope`.
    fn resolve(&self, scope: usize, name: &str) -> Resolved {
        let mut at = scope;
        loop {
            let this = &self.scopes[at];
            // A class body is no scope of what is inside it, but its own
            if this.kind != Kind::Class || at == scope {
                if this.global.contains(name) && at != 0 {
                    at = 0;
                    continue;
                }
                if !this.nonlocal.contains(name)
                    && let Some(bound) = this.bindings.get(name)
                {
                    let imports: Vec<String> = bound
                        .iter()
                        .filter_map(|binding| match binding {
                            Binding::Import(target) => Some(target.clone()),
                            Binding::Local => None,
                        })
                        .collect();
                    return if imports.is_empty() {
                        Resolved::Own
                    } else {
                        Resolved::Imports(imports)
                    };
                }
            }
            match this.parent {
                Some(parent) => at = parent,
                None => return Resolved::Unbound,
            }
        }
    }

    /// Resolve every use, now that every scope is known.
    fn finish(self) -> Found {
        let mut uses = Vec::new();
        let mut dynamic = Vec::new();
        let mut unread = Vec::new();
        for entry in &self.entries {
            let pending = match entry {
                Entry::Resolved(used) => {
                    uses.push(used.clone());
                    continue;
                }
                Entry::Pending(pending) => pending,
            };
            let Some((first, rest)) = pending.chain.split_first() else {
                continue;
            };
            let names: Vec<String> = match self.resolve(pending.scope, first) {
                Resolved::Imports(targets) => targets
                    .into_iter()
                    .map(|target| {
                        std::iter::once(target.as_str())
                            .chain(rest.iter().map(String::as_str))
                            .collect::<Vec<_>>()
                            .join(".")
                    })
                    .collect(),
                Resolved::Unbound if is_builtin(first) => {
                    vec![format!("builtins.{}", pending.chain.join("."))]
                }
                Resolved::Unbound | Resolved::Own => continue,
            };
            for name in names {
                // Code that is run from what is no code a reader can read, whether it is called or handed on
                if RUNS_CODE.contains(&name.as_str()) && !pending.in_type {
                    unread.push((pending.line, name.clone()));
                }
                if let Some(call) = &pending.call
                    && IMPORTS.contains(&name.as_str())
                {
                    match &call.first {
                        Argument::Literal(module) => {
                            let parts: Vec<String> = module.split('.').map(String::from).collect();
                            // A relative name (`".x"`) has an empty part: it needs the package it is taken from
                            if parts.iter().all(|part| !part.is_empty()) {
                                dynamic.push((call.arguments_line, parts));
                            } else {
                                unread.push((call.arguments_line, name.clone()));
                            }
                        }
                        Argument::Other => unread.push((call.arguments_line, name.clone())),
                        Argument::Absent => {}
                    }
                }
                let how = match &pending.call {
                    Some(call) => How::Called {
                        positional: call.positional,
                        keywords: call.keywords.clone(),
                        unpacked: call.unpacked,
                        first: call.lead,
                    },
                    None => How::Value,
                };
                uses.push(Use {
                    line: pending.line,
                    name,
                    how,
                    in_type: pending.in_type,
                });
            }
        }
        // In the order of the lines, the order the file is read in among those of one line
        uses.sort_by_key(|used| used.line);
        unread.sort_by_key(|(line, _)| *line);
        let mut methods: Vec<Method> = self
            .methods
            .iter()
            .map(|pending| Method {
                line: pending.line,
                name: pending.name.clone(),
                resolved: self.callee(pending),
                positional: pending.call.positional,
                keywords: pending.call.keywords.clone(),
                unpacked: pending.call.unpacked,
                first: pending.call.lead,
            })
            .collect();
        methods.sort_by_key(|method| method.line);
        Found {
            uses,
            dynamic,
            unread,
            methods,
            defined: self.defined,
        }
    }

    /// What the callee of a call of an attribute resolves to, when its chain starts at an import or at a builtin.
    fn callee(&self, pending: &PendingMethod) -> Vec<String> {
        let Some((first, rest)) = pending.chain.as_ref().and_then(|chain| chain.split_first())
        else {
            return Vec::new();
        };
        match self.resolve(pending.scope, first) {
            Resolved::Imports(targets) => targets
                .into_iter()
                .map(|target| {
                    std::iter::once(target.as_str())
                        .chain(rest.iter().map(String::as_str))
                        .collect::<Vec<_>>()
                        .join(".")
                })
                .collect(),
            Resolved::Unbound if is_builtin(first) => {
                let whole = std::iter::once(first.as_str())
                    .chain(rest.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(".");
                vec![format!("builtins.{whole}")]
            }
            Resolved::Unbound | Resolved::Own => Vec::new(),
        }
    }
}

/// The names of a chain of attributes that starts at a name, and where the chain starts; `None` for anything else
/// (`f().x`, `a[0].x`).
fn chain(expr: &Expr) -> Option<(Vec<String>, usize)> {
    match expr {
        Expr::Name(name) => Some((vec![name.id.to_string()], name.range.start().to_usize())),
        Expr::Attribute(attribute) => {
            let (mut names, at) = chain(&attribute.value)?;
            names.push(attribute.attr.to_string());
            Some((names, at))
        }
        _ => None,
    }
}

impl<'a> Visitor<'a> for Reader<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => self.function(def),
            Stmt::ClassDef(class) => self.class(class),
            Stmt::Import(_) | Stmt::ImportFrom(_) => self.import(stmt),
            Stmt::Global(global) => {
                for name in &global.names {
                    self.scopes[self.current].global.insert(name.to_string());
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    self.scopes[self.current].nonlocal.insert(name.to_string());
                }
            }
            Stmt::TypeAlias(alias) => {
                let outer = self.current;
                self.type_params(alias.type_params.as_deref());
                let was = replace(&mut self.in_type, true);
                self.visit_expr(&alias.value);
                self.in_type = was;
                self.current = outer;
                self.visit_expr(&alias.name);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        let was = replace(&mut self.in_type, true);
        self.visit_expr(expr);
        self.in_type = was;
    }

    fn visit_type_param(&mut self, param: &'a TypeParam) {
        let was = replace(&mut self.in_type, true);
        ruff_python_ast::visitor::walk_type_param(self, param);
        self.in_type = was;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match name.ctx {
                ExprContext::Load => {
                    self.record(
                        vec![name.id.to_string()],
                        name.range.start().to_usize(),
                        None,
                    );
                }
                ExprContext::Store | ExprContext::Del => {
                    self.bind(name.id.as_str(), Binding::Local)
                }
                ExprContext::Invalid => {}
            },
            Expr::Attribute(attribute) => match chain(expr) {
                Some((names, at)) => self.record(names, at, None),
                None => self.visit_expr(&attribute.value),
            },
            Expr::Call(call) => {
                // A call runs when the annotation it is in is evaluated: it, its callee and its arguments are no type
                let was = replace(&mut self.in_type, false);
                if let Expr::Attribute(attribute) = call.func.as_ref() {
                    let pending = PendingMethod {
                        scope: self.current,
                        chain: chain(&call.func).map(|(names, _)| names),
                        name: attribute.attr.to_string(),
                        line: self.line(attribute.attr.range.start().to_usize()),
                        call: self.call(&call.arguments),
                    };
                    self.methods.push(pending);
                }
                match chain(&call.func) {
                    Some((names, at)) => {
                        let info = self.call(&call.arguments);
                        self.record(names, at, Some(info));
                    }
                    None => self.visit_expr(&call.func),
                }
                self.visit_arguments(&call.arguments);
                self.in_type = was;
            }
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = named.target.as_ref() {
                    let scope = self.walrus_scope();
                    self.bind_in(scope, target.id.as_str(), Binding::Local);
                }
            }
            Expr::Lambda(lambda) => {
                let outer = self.current;
                if let Some(parameters) = &lambda.parameters {
                    self.defaults(parameters);
                }
                self.push(Kind::Function);
                if let Some(parameters) = &lambda.parameters {
                    self.bind_parameters(parameters);
                }
                self.visit_expr(&lambda.body);
                self.current = outer;
            }
            Expr::ListComp(comp) => {
                self.comprehension(&comp.generators, |reader| reader.visit_expr(&comp.elt));
            }
            Expr::SetComp(comp) => {
                self.comprehension(&comp.generators, |reader| reader.visit_expr(&comp.elt));
            }
            Expr::Generator(comp) => {
                self.comprehension(&comp.generators, |reader| reader.visit_expr(&comp.elt));
            }
            Expr::DictComp(comp) => {
                self.comprehension(&comp.generators, |reader| {
                    if let Some(key) = &comp.key {
                        reader.visit_expr(key);
                    }
                    reader.visit_expr(&comp.value);
                });
            }
            _ => ruff_python_ast::visitor::walk_expr(self, expr),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(exception) = &handler.type_ {
            self.visit_expr(exception);
        }
        if let Some(name) = &handler.name {
            self.bind(name.as_str(), Binding::Local);
        }
        self.visit_body(&handler.body);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        let captured = match pattern {
            Pattern::MatchAs(capture) => capture.name.as_ref(),
            Pattern::MatchStar(capture) => capture.name.as_ref(),
            Pattern::MatchMapping(mapping) => mapping.rest.as_ref(),
            _ => None,
        };
        if let Some(name) = captured {
            self.bind(name.as_str(), Binding::Local);
        }
        walk_pattern(self, pattern);
    }
}

#[cfg(test)]
mod tests {
    use domain::code::{First, How, Source, Use};

    use crate::outbound::python::read;

    fn shown(used: &Use) -> String {
        let how = match &used.how {
            How::Imported => "import".to_string(),
            How::Star => "star".into(),
            How::Called {
                positional,
                keywords,
                unpacked,
                ..
            } => {
                let keywords = if keywords.is_empty() {
                    String::new()
                } else {
                    format!(" {}", keywords.join(","))
                };
                let unpacked = if *unpacked { " *" } else { "" };
                format!("call {positional}{keywords}{unpacked}")
            }
            How::Value => "value".into(),
            How::Constructed { .. } | How::TypeOf => {
                unreachable!("only TypeScript's reader gives these")
            }
        };
        let place = if used.in_type { " type" } else { "" };
        format!("{}:{}:{how}{place}", used.line, used.name)
    }

    fn all(source: &str) -> Source<Vec<String>> {
        read(source, "app/main.py")
    }

    /// Every use but the imports and the builtins, as `line:name:how`.
    fn used(source: &str) -> Vec<String> {
        all(source)
            .uses
            .iter()
            .filter(|used| !matches!(used.how, How::Imported | How::Star))
            .filter(|used| !used.name.starts_with("builtins."))
            .map(shown)
            .collect()
    }

    fn nothing() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn a_name_resolves_through_the_import_that_binds_it() {
        let source = "\
import os
import os.path
import a.b as c
from d import e as f
from g import h
os.getenv(\"X\")
c.run()
f.run()
h.run()
os.path.exists(1)
";
        assert_eq!(
            used(source),
            [
                "6:os.getenv:call 1",
                "7:a.b.run:call 0",
                "8:d.e.run:call 0",
                "9:g.h.run:call 0",
                "10:os.path.exists:call 1",
            ]
        );
    }

    #[test]
    fn a_name_taken_by_from_import_is_a_use_at_the_import_and_a_relative_one_is_resolved() {
        let source = "\
from os import environ, getenv as g
from . import theme
from ..molecules.row import Row
from ... import beyond
from typing import *
";
        let imported: Vec<String> = read(source, "ui/atoms/button.py")
            .uses
            .iter()
            .map(shown)
            .collect();
        assert_eq!(
            imported,
            [
                "1:os.environ:import",
                "1:os.getenv:import",
                "2:ui.atoms.theme:import",
                "3:ui.molecules.row.Row:import",
                "5:typing:star",
            ]
        );
    }

    #[test]
    fn a_chain_goes_as_far_as_names_and_not_through_a_call_or_a_subscript() {
        let source = "\
import os
import json
x = os.path.join(a, b).strip()
y = os.environ[\"A\"].lower()
z = json.loads(os.environ.get(\"B\")).keys
";
        assert_eq!(
            used(source),
            [
                "3:os.path.join:call 2",
                "4:os.environ:value",
                "5:json.loads:call 1",
                "5:os.environ.get:call 1",
            ]
        );
    }

    #[test]
    fn a_call_says_how_many_arguments_it_has_and_which_keywords() {
        let source = "\
import time
from dataclasses import field
from datetime import datetime
f = time.time
g = field(default_factory=datetime.now, init=False)
h = time.sleep(1, 2, *rest)
i = datetime.now(**kw)
j = time.time()
";
        assert_eq!(
            used(source),
            [
                "4:time.time:value",
                "5:dataclasses.field:call 0 default_factory,init",
                "5:datetime.datetime.now:value",
                "6:time.sleep:call 2 *",
                "7:datetime.datetime.now:call 0 *",
                "8:time.time:call 0",
            ]
        );
    }

    #[test]
    fn the_module_itself_is_a_value() {
        let source = "\
import os
import importlib.util
o = os
check(os)
getattr(os, \"environ\")
m = importlib.util
";
        assert_eq!(
            used(source),
            [
                "3:os:value",
                "4:os:value",
                "5:os:value",
                "6:importlib.util:value",
            ]
        );
    }

    #[test]
    fn a_decorator_and_a_base_class_are_values_and_a_called_decorator_is_a_call() {
        let source = "\
import abc
import functools
@functools.cache
def f(): pass
@functools.lru_cache(maxsize=1)
def g(): pass
class C(abc.ABC, metaclass=abc.ABCMeta):
    pass
";
        assert_eq!(
            used(source),
            [
                "3:functools.cache:value",
                "5:functools.lru_cache:call 0 maxsize",
                "7:abc.ABC:value",
                "7:abc.ABCMeta:value",
            ]
        );
    }

    #[test]
    fn a_parameter_or_a_variable_of_the_name_is_not_the_module() {
        let source = "\
import os
def f(os):
    return os.environ
def g(a, *os):
    return os.environ
def h(**os):
    return os.environ
def i():
    os = 1
    return os.environ
def j():
    for os in range(3):
        os.environ
def k():
    with open(\"f\") as os:
        os.environ
def l():
    try:
        pass
    except Exception as os:
        os.environ
def m():
    os = 1
    del os
    return os.environ
def n():
    return os.environ
";
        assert_eq!(used(source), ["27:os.environ:value"]);
    }

    #[test]
    fn a_name_bound_anywhere_in_a_function_is_local_in_all_of_it() {
        let source = "\
import os
def f():
    os.getenv(\"A\")
    os = 1
def g():
    os.getenv(\"B\")
    import os
";
        assert_eq!(used(source), ["6:os.getenv:call 1"]);
    }

    #[test]
    fn an_import_in_a_function_binds_there_and_not_outside() {
        let source = "\
def f():
    import os
    return os.getenv(\"A\")
def g():
    return os.getenv(\"B\")
os.getenv(\"C\")
";
        assert_eq!(used(source), ["3:os.getenv:call 1"]);
    }

    #[test]
    fn a_class_body_binds_for_itself_and_not_for_the_functions_inside_it() {
        let source = "\
import os
class C:
    os = 1
    x = os.environ
    def m(self):
        return os.getenv(\"A\")
class D:
    import json
    y = json.loads(\"1\")
    def m(self):
        return json.dumps(1)
    z = [json.loads(s) for s in \"a\"]
";
        // Not line 11, a method of `D`, and not line 12: the comprehension sits in the class body, which does not
        // enclose it, so `json` is not its
        assert_eq!(used(source), ["6:os.getenv:call 1", "9:json.loads:call 1"]);
    }

    #[test]
    fn global_binds_and_reads_in_the_module_and_nonlocal_in_the_enclosing_function() {
        let source = "\
def setup():
    global os
    import os
def f():
    return os.getenv(\"A\")
def outer():
    os = make()
    def inner():
        nonlocal os
        return os.environ
    return inner
def outer2():
    import json
    def inner():
        nonlocal json
        return json.dumps(1)
    return inner
";
        assert_eq!(used(source), ["5:os.getenv:call 1", "16:json.dumps:call 1"]);
    }

    #[test]
    fn a_comprehension_has_a_scope_and_its_first_iterable_is_evaluated_outside_it() {
        let source = "\
import os
a = [os for os in range(3)]
b = [x for os in os.listdir() for x in os.environ]
c = {os: 1 for os in range(3) if os.environ}
d = (os.getenv(x) for x in range(3))
e = [x for x in os.environ for os in range(2)]
";
        assert_eq!(
            used(source),
            [
                "3:os.listdir:call 0",
                "5:os.getenv:call 1",
                "6:os.environ:value",
            ]
        );
    }

    #[test]
    fn a_walrus_binds_in_the_nearest_scope_that_is_no_comprehension() {
        let source = "\
import os
def f():
    if (os := get()):
        return os.environ
def g():
    [os := get(i) for i in range(2)]
    return os.environ
h = [os := get(i) for i in range(2)]
";
        assert_eq!(used(source), nothing());
        let source = "\
import os
def f():
    [x := os.getenv(i) for i in range(2)]
    return x.environ
";
        assert_eq!(used(source), ["3:os.getenv:call 1"]);
    }

    #[test]
    fn a_capture_of_match_binds() {
        let source = "\
import os
def f(x):
    match x:
        case [os, *rest]:
            return os.environ
def g(x):
    match x:
        case {\"a\": 1, **os}:
            return os.environ
def h(x):
    match x:
        case Point(x=1) as os:
            return os.environ
def i(x):
    match x:
        case os.PathLike():
            return os.getenv(\"A\")
";
        assert_eq!(
            used(source),
            ["16:os.PathLike:value", "17:os.getenv:call 1"]
        );
    }

    #[test]
    fn a_lambda_has_its_own_scope_and_evaluates_its_defaults_outside() {
        let source = "\
import os
a = lambda os: os.environ
b = lambda: os.environ
c = lambda x=os.getenv(\"A\"): x
d = lambda os=os: os.environ
";
        assert_eq!(
            used(source),
            ["3:os.environ:value", "4:os.getenv:call 1", "5:os:value",]
        );
    }

    #[test]
    fn a_function_evaluates_its_defaults_and_annotations_outside_its_scope() {
        let source = "\
import os
def f(os=os.curdir, b: os.PathLike = None) -> os.PathLike:
    return os
def g(os, /, b: os.PathLike = None):
    return os
";
        assert_eq!(
            used(source),
            [
                "2:os.curdir:value",
                "2:os.PathLike:value type",
                "2:os.PathLike:value type",
                "4:os.PathLike:value type",
            ]
        );
    }

    #[test]
    fn type_parameters_have_a_scope_between_the_definition_and_its_body() {
        let source = "\
import os
def f[T: os.PathLike](x: T) -> T:
    return os.getenv(\"A\")
def g[os](x: os):
    return os.environ
type Alias[S] = dict[S, os.PathLike]
class C[T](os.PathLike):
    pass
";
        assert_eq!(
            used(source),
            [
                "2:os.PathLike:value type",
                "3:os.getenv:call 1",
                "6:os.PathLike:value type",
                "7:os.PathLike:value",
            ]
        );
    }

    #[test]
    fn an_import_under_type_checking_is_any_import() {
        let source = "\
from typing import TYPE_CHECKING
if TYPE_CHECKING:
    import os
else:
    import json as os
def f():
    return os.getenv(\"A\")
";
        assert_eq!(
            used(source),
            [
                "2:typing.TYPE_CHECKING:value",
                "7:os.getenv:call 1",
                "7:json.getenv:call 1"
            ]
        );
    }

    #[test]
    fn a_name_two_imports_bind_is_used_as_each() {
        let source = "\
try:
    import simplejson as json
except ImportError:
    import json
json.dumps(1)
";
        assert_eq!(
            used(source),
            ["5:simplejson.dumps:call 1", "5:json.dumps:call 1"]
        );
    }

    #[test]
    fn a_call_in_an_annotation_and_all_it_holds_is_no_type_position() {
        let source = "\
import uuid
from datetime import datetime
from typing import Annotated
from pydantic import Field, Depends
from pathlib import Path
from app import Session, get_db
class Model:
    created: Annotated[datetime, Field(default_factory=datetime.now)]
    ident: Annotated[str, Field(default_factory=uuid.uuid4)]
    third: Annotated[str, Field(default_factory=lambda: uuid.uuid4().hex)]
def f(db: Annotated[Session, Depends(get_db)], p: Path = None) -> list[Path]:
    pass
";
        let found: Vec<String> = all(source)
            .uses
            .iter()
            .filter(|used| !matches!(used.how, How::Imported))
            .map(shown)
            .collect();
        assert_eq!(
            found,
            [
                "8:typing.Annotated:value type",
                "8:datetime.datetime:value type",
                "8:pydantic.Field:call 0 default_factory",
                "8:datetime.datetime.now:value",
                "9:typing.Annotated:value type",
                "9:builtins.str:value type",
                "9:pydantic.Field:call 0 default_factory",
                "9:uuid.uuid4:value",
                "10:typing.Annotated:value type",
                "10:builtins.str:value type",
                "10:pydantic.Field:call 0 default_factory",
                "10:uuid.uuid4:call 0",
                "11:typing.Annotated:value type",
                "11:app.Session:value type",
                "11:pydantic.Depends:call 1",
                "11:app.get_db:value",
                "11:pathlib.Path:value type",
                "11:builtins.list:value type",
                "11:pathlib.Path:value type",
            ]
        );
    }

    #[test]
    fn a_use_in_an_annotation_is_in_a_type_position() {
        let source = "\
import os
from pathlib import Path
def f(p: os.PathLike, q: list[Path]) -> dict[str, os.PathLike]:
    x: dict[str, os.PathLike] = {}
    y: int
    return os.getcwd()
z: Path = Path.home()
";
        assert_eq!(
            used(source),
            [
                "3:os.PathLike:value type",
                "3:pathlib.Path:value type",
                "3:os.PathLike:value type",
                "4:os.PathLike:value type",
                "6:os.getcwd:call 0",
                // The value of an assignment is evaluated before its annotation
                "7:pathlib.Path.home:call 0",
                "7:pathlib.Path:value type",
            ]
        );
    }

    #[test]
    fn a_builtin_no_scope_binds_is_a_use_of_builtins() {
        let source = "\
import builtins
open(\"f\")
print(1, end=\"\")
x = input
getattr(os, \"environ\")
builtins.open(\"f\")
str.join(\",\", [])
";
        let found: Vec<String> = all(source).uses.iter().map(shown).collect();
        assert_eq!(
            found,
            [
                "2:builtins.open:call 1",
                "3:builtins.print:call 1 end",
                "4:builtins.input:value",
                "5:builtins.getattr:call 2",
                "6:builtins.open:call 1",
                "7:builtins.str.join:call 2",
            ]
        );
    }

    #[test]
    fn a_builtin_that_a_scope_binds_is_not_one() {
        let source = "\
def f(open):
    return open(\"f\")
def g():
    print = log
    print(1)
input = ask
input()
class C:
    def open(self):
        pass
    x = open
from os import open
open()
";
        let found: Vec<String> = all(source)
            .uses
            .iter()
            .filter(|used| !matches!(used.how, How::Imported))
            .map(shown)
            .collect();
        // The module binds `open` by `from os import open`, which `f`'s own parameter shadows; the class's own `open`
        // is the one `x = open` takes, and `print` and `input` are bound by assignments
        assert_eq!(found, ["13:os.open:call 0"]);
    }

    #[test]
    fn a_star_import_is_recorded_and_the_names_it_brings_in_are_no_uses() {
        let source = "\
from os import *
from pathlib import *
getenv(\"A\")
Path.home()
open(\"f\")
";
        let found: Vec<String> = all(source).uses.iter().map(shown).collect();
        // A builtin is one still: the star import may have shadowed it, and the star is the finding
        assert_eq!(
            found,
            ["1:os:star", "2:pathlib:star", "5:builtins.open:call 1"]
        );
    }

    #[test]
    fn a_name_that_nothing_binds_and_is_no_builtin_is_no_use() {
        assert_eq!(
            used("x = unknown.attr\nunknown()\nself.os.environ\n"),
            nothing()
        );
    }

    #[test]
    fn a_use_in_an_fstring_a_subscript_and_a_with_is_found() {
        let source = "\
import os
a = f\"{os.getenv('A')!r:>{os.curdir}}\"
with os.scandir(\".\") as entries:
    pass
b = {os.sep: 1}
c = [*os.environ]
del os.environ[\"A\"]
os.environ = {}
";
        assert_eq!(
            used(source),
            [
                "2:os.getenv:call 1",
                "2:os.curdir:value",
                "3:os.scandir:call 1",
                "5:os.sep:value",
                "6:os.environ:value",
                "7:os.environ:value",
                "8:os.environ:value",
            ]
        );
    }

    #[test]
    fn the_uses_come_in_the_order_of_the_lines() {
        let source = "\
import os
def f():
    return os.getenv(\"A\")
x = os.getcwd()
";
        let lines: Vec<usize> = all(source).uses.iter().map(|used| used.line).collect();
        assert_eq!(lines, [3, 4]);
    }

    #[test]
    fn a_call_that_imports_by_a_name_built_at_run_time_is_unread() {
        let source = "\
import importlib
name = \"x\"
importlib.import_module(name)
importlib.import_module(f\"billing.{name}\")
importlib.import_module(\".x\", package=\"billing\")
importlib.import_module()
__import__(*[\"billing\"])
__import__(\"billing.domain\")
other.__import__(\"billing.domain.x\")
";
        let read = all(source);
        assert_eq!(
            read.unread,
            [
                (3, "importlib.import_module".to_string()),
                (4, "importlib.import_module".into()),
                (5, "importlib.import_module".into()),
                (7, "builtins.__import__".into()),
            ]
        );
        let imported: Vec<(usize, String)> = read
            .imports
            .iter()
            .map(|(line, module)| (*line, module.join(".")))
            .collect();
        assert_eq!(
            imported,
            [(1, "importlib".to_string()), (8, "billing.domain".into())]
        );
    }

    #[test]
    fn a_function_the_file_binds_itself_is_not_an_import_by_a_call() {
        let source = "\
def __import__(name):
    pass
def f(importlib):
    importlib.import_module(\"billing\")
__import__(\"billing.domain\")
";
        let read = all(source);
        assert!(read.unread.is_empty());
        assert!(read.imports.is_empty());
    }

    #[test]
    fn eval_and_exec_are_unread_called_or_taken_as_a_value_unless_the_file_binds_them() {
        let source = "\
eval(\"1\")
exec(code)
f = eval
import builtins
builtins.exec(\"x\")
def g(eval):
    return eval(\"y\")
class C:
    def exec(self):
        pass
x: eval
";
        let read = all(source);
        assert_eq!(
            read.unread,
            [
                (1, "builtins.eval".to_string()),
                (2, "builtins.exec".into()),
                (3, "builtins.eval".into()),
                (5, "builtins.exec".into()),
            ]
        );
    }

    /// A method as `line:name(resolved) positional keywords flags`.
    fn called_attributes(source: &str) -> Vec<String> {
        all(source)
            .methods
            .iter()
            .map(|method| {
                let keywords = if method.keywords.is_empty() {
                    String::new()
                } else {
                    format!(" {}", method.keywords.join(","))
                };
                let unpacked = if method.unpacked { " *" } else { "" };
                let first = match method.first {
                    First::Other => "",
                    First::Null => " none",
                    First::Text => " text",
                };
                format!(
                    "{}:{}({}) {}{keywords}{unpacked}{first}",
                    method.line,
                    method.name,
                    method.resolved.join("|"),
                    method.positional
                )
            })
            .collect()
    }

    #[test]
    fn a_call_of_an_attribute_is_a_method_whatever_it_is_the_attribute_of() {
        let source = "\
import os
from pathlib import Path
def f(p, self):
    p.read_text()
    Path(\"a\").exists()
    self.repo.mkdir(\"x\", parents=True)
    os.path.exists(p)
    (a or b).stat()
    items[0].unlink()
    f().touch()
    open(\"a\").read()
";
        assert_eq!(
            called_attributes(source),
            [
                "4:read_text() 0",
                "5:exists() 0",
                "6:mkdir() 1 parents text",
                "7:exists(os.path.exists) 1",
                "8:stat() 0",
                "9:unlink() 0",
                "10:touch() 0",
                // The attribute of the result of a call resolves to nothing, whatever the call is
                "11:read() 0",
            ]
        );
    }

    #[test]
    fn a_method_says_how_many_arguments_it_has_and_what_the_first_is() {
        let source = "\
p.open(\"w\")
p.open('r', encoding=\"utf8\")
p.open(mode=\"w\")
p.gmtime(None)
p.gmtime(x)
p.gmtime(*a)
p.gmtime(**kw)
p.open(f\"{x}\")
p.open(*a, \"w\")
";
        assert_eq!(
            called_attributes(source),
            [
                "1:open() 1 text",
                "2:open() 1 encoding text",
                "3:open() 0 mode",
                "4:gmtime() 1 none",
                "5:gmtime() 1",
                "6:gmtime() 0 *",
                "7:gmtime() 0 *",
                "8:open() 1",
                "9:open() 1 *",
            ]
        );
    }

    #[test]
    fn a_method_takes_its_callee_from_the_scopes_of_the_file() {
        let source = "\
import time as t
from datetime import datetime
def f(t):
    t.time()
def g():
    return datetime.now().isoformat(), t.time()
";
        // `t` of `f` is a parameter; in `g` it is `time`, and `datetime.now()` resolves to the clock
        assert_eq!(
            called_attributes(source),
            [
                "4:time() 0",
                "6:isoformat() 0",
                "6:now(datetime.datetime.now) 0",
                "6:time(time.time) 0",
            ]
        );
    }

    /// The methods a class defines, as `name/parameters`.
    fn defined(source: &str) -> Vec<String> {
        all(source)
            .defined
            .iter()
            .map(|method| format!("{}/{}", method.name, method.parameters))
            .collect()
    }

    #[test]
    fn a_method_a_class_defines_has_its_parameters_counted_without_self() {
        let source = "\
class A:
    def exists(self): ...
    def read_text(self, encoding=None, *, errors=None): ...
    def many(self, *args, **kwargs): ...
    async def fetch(self, key): ...
    @staticmethod
    def make(a, b): ...
    @classmethod
    def build(cls, a): ...
    @property
    def size(self): ...
    def noself(): ...
    class Inner:
        def deep(self, a): ...
    def outer(self):
        def nested(a, b): ...
def free(a, b): ...
";
        assert_eq!(
            defined(source),
            [
                "exists/0",
                "read_text/2",
                "many/2",
                "fetch/1",
                "make/2",
                "build/1",
                "size/0",
                "noself/0",
                "deep/1",
                "outer/0",
            ]
        );
    }
}
