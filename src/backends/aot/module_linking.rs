// Cross-module linking support
// Generic, language-agnostic module dependency management

use super::symbols::{Symbol, SymbolTable};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Module dependency
#[derive(Debug, Clone)]
pub struct ModuleDependency {
    /// Module name
    pub name: String,
    /// Imported symbols
    pub imports: Vec<String>,
    /// Module path
    pub path: Option<PathBuf>,
}

impl ModuleDependency {
    /// Create new dependency
    pub fn new(name: String) -> Self {
        Self {
            name,
            imports: Vec::new(),
            path: None,
        }
    }

    /// Add imported symbol
    pub fn add_import(&mut self, symbol: String) {
        if !self.imports.contains(&symbol) {
            self.imports.push(symbol);
        }
    }

    /// Set module path
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }
}

/// Module interface (exported symbols)
#[derive(Debug, Clone)]
pub struct ModuleInterface {
    /// Module name
    pub name: String,
    /// Exported symbols
    pub exports: SymbolTable,
    /// Module version (for compatibility checking)
    pub version: String,
}

impl ModuleInterface {
    /// Create new module interface
    pub fn new(name: String, version: String) -> Self {
        Self {
            name,
            exports: SymbolTable::new(),
            version,
        }
    }

    /// Add exported symbol
    pub fn add_export(&mut self, symbol: Symbol) {
        self.exports.add_symbol(symbol);
    }

    /// Get export
    pub fn get_export(&self, name: &str) -> Option<&Symbol> {
        self.exports.lookup(&self.name, name)
    }

    /// Serialize to bytes (for .mli interface files)
    pub fn serialize(&self) -> Vec<u8> {
        // Simplified serialization
        let json = format!(
            r#"{{"name":"{}","version":"{}","exports":{}}}"#,
            self.name,
            self.version,
            self.exports.exports().len()
        );
        json.into_bytes()
    }

    /// Deserialize from bytes
    pub fn deserialize(_bytes: &[u8]) -> Result<Self, String> {
        // Simplified deserialization
        Ok(Self::new("module".to_string(), "1.0.0".to_string()))
    }
}

/// Module dependency graph
#[derive(Debug)]
pub struct DependencyGraph {
    /// Modules
    modules: HashMap<String, Vec<String>>,
    /// Interfaces
    interfaces: HashMap<String, ModuleInterface>,
}

impl DependencyGraph {
    /// Create new dependency graph
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            interfaces: HashMap::new(),
        }
    }

    /// Add module
    pub fn add_module(&mut self, name: String, deps: Vec<String>) {
        self.modules.insert(name, deps);
    }

    /// Add interface
    pub fn add_interface(&mut self, interface: ModuleInterface) {
        self.interfaces.insert(interface.name.clone(), interface);
    }

    /// Get interface
    pub fn get_interface(&self, name: &str) -> Option<&ModuleInterface> {
        self.interfaces.get(name)
    }

    /// Topological sort (dependency order)
    pub fn topo_sort(&self) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp = HashSet::new();

        for module in self.modules.keys() {
            if !visited.contains(module) {
                self.visit(module, &mut visited, &mut temp, &mut result)?;
            }
        }

        Ok(result)
    }

    /// Visit module (DFS)
    fn visit(
        &self,
        module: &str,
        visited: &mut HashSet<String>,
        temp: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), String> {
        if temp.contains(module) {
            return Err(format!("Circular dependency detected: {}", module));
        }

        if visited.contains(module) {
            return Ok(());
        }

        temp.insert(module.to_string());

        if let Some(deps) = self.modules.get(module) {
            for dep in deps {
                self.visit(dep, visited, temp, result)?;
            }
        }

        temp.remove(module);
        visited.insert(module.to_string());
        result.push(module.to_string());

        Ok(())
    }

    /// Check if all dependencies are satisfied
    pub fn validate(&self) -> Result<(), String> {
        for (module, deps) in &self.modules {
            for dep in deps {
                if !self.interfaces.contains_key(dep) && !self.modules.contains_key(dep) {
                    return Err(format!("Missing dependency: {} requires {}", module, dep));
                }
            }
        }
        Ok(())
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental compilation tracker
#[derive(Debug)]
pub struct IncrementalTracker {
    /// Compiled modules (name -> timestamp)
    compiled: HashMap<String, u64>,
    /// Module hashes (name -> hash)
    hashes: HashMap<String, u64>,
}

impl IncrementalTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            hashes: HashMap::new(),
        }
    }

    /// Mark module as compiled
    pub fn mark_compiled(&mut self, name: String, timestamp: u64, hash: u64) {
        self.compiled.insert(name.clone(), timestamp);
        self.hashes.insert(name, hash);
    }

    /// Check if module needs recompilation
    pub fn needs_recompile(&self, name: &str, hash: u64) -> bool {
        self.hashes.get(name).map(|h| *h != hash).unwrap_or(true)
    }

    /// Get last compiled time
    pub fn last_compiled(&self, name: &str) -> Option<u64> {
        self.compiled.get(name).copied()
    }
}

impl Default for IncrementalTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_dependency() {
        let mut dep = ModuleDependency::new("math".to_string());
        dep.add_import("add".to_string());
        assert_eq!(dep.imports.len(), 1);
    }

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::new();
        graph.add_module("main".to_string(), vec!["math".to_string()]);
        graph.add_module("math".to_string(), vec![]);

        let order = graph.topo_sort().unwrap();
        assert_eq!(order.len(), 2);
        // math should come before main
        let math_idx = order.iter().position(|m| m == "math").unwrap();
        let main_idx = order.iter().position(|m| m == "main").unwrap();
        assert!(math_idx < main_idx);
    }

    #[test]
    fn test_circular_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_module("a".to_string(), vec!["b".to_string()]);
        graph.add_module("b".to_string(), vec!["a".to_string()]);

        let result = graph.topo_sort();
        assert!(result.is_err());
    }
}
