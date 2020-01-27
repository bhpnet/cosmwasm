use wasm_nm::{Options, Symbol, Symbols};

use crate::errors::{Result, ValidationErr};

static PUBLIC_SYMBOLS: Options = Options {
    imports: true,
    exports: true,
    privates: false,
    sizes: false,
};

/// Lists all imports we provide upon instantiating the instance in Instance::from_module()
/// This should be updated when new imports are added
static SUPPORTED_IMPORTS: &[&str] = &[
    "read_db",
    "write_db",
    "canonicalize_address",
    "humanize_address",
];

/// Lists all entry points we expect to be present when calling a contract.
/// Basically, anything that is used in calls.rs
/// This is unlikely to change much, must be frozen at 1.0 to avoid breaking existing contracts
static REQUIRED_EXPORTS: &[&str] = &[
    "query",
    "init",
    "handle",
    "allocate",
    "deallocate",
    "cosmwasm_api_0_6",
];

static EXTRA_IMPORT_MSG: &str = "WASM requires unsupported imports - version too new?";

static MISSING_EXPORT_MSG: &str = "WASM doesn't have required exports - version too old?";

pub fn check_api_compatibility(wasm_code: &[u8]) -> Result<()> {
    let mut reader = std::io::Cursor::new(wasm_code);
    let symbols = wasm_nm::symbols(PUBLIC_SYMBOLS.clone(), &mut reader).unwrap();
    if !import_requirements_satisfied(&symbols, SUPPORTED_IMPORTS) {
        return ValidationErr {
            msg: EXTRA_IMPORT_MSG,
        }
        .fail();
    }
    if !has_all_exports(&symbols, REQUIRED_EXPORTS) {
        return ValidationErr {
            msg: MISSING_EXPORT_MSG,
        }
        .fail();
    }
    Ok(())
}

/// Checks if the import requirements of the contract are satisfied.
/// When this is not the case, we either have an incompatibility between contract and VM
/// or a error in the contract.
fn import_requirements_satisfied(symbols: &Symbols, supported_imports: &[&str]) -> bool {
    let required_imports: Vec<&str> = symbols
        .iter()
        .filter_map(|s| match s {
            Symbol::Import { name } => Some(name),
            _ => None,
        })
        .collect();

    for required_import in required_imports {
        if !supported_imports.contains(&required_import) {
            return false;
        }
    }
    true
}

fn has_all_exports(symbols: &Symbols, required: &[&str]) -> bool {
    let exports: Vec<&str> = symbols
        .iter()
        .filter_map(|s| match s {
            Symbol::Export { name, .. } => Some(name),
            _ => None,
        })
        .collect();

    for i in required {
        if !exports.contains(&i) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod test {
    use super::*;

    static CONTRACT_0_6: &[u8] = include_bytes!("../testdata/contract_0.6.wasm");
    static CONTRACT_0_7: &[u8] = include_bytes!("../testdata/contract_0.7.wasm");

    #[test]
    fn test_supported_imports() {
        let mut reader = std::io::Cursor::new(CONTRACT_0_6);
        let symbols = wasm_nm::symbols(PUBLIC_SYMBOLS.clone(), &mut reader).unwrap();

        // if contract has more than we provide, bad
        let imports_good = import_requirements_satisfied(&symbols, &["c_read", "c_write"]);
        assert_eq!(imports_good, false);

        // exact match good
        let imports_good = import_requirements_satisfied(
            &symbols,
            &[
                "c_read",
                "c_write",
                "c_canonical_address",
                "c_human_address",
            ],
        );
        assert_eq!(imports_good, true);

        // if we provide more, also good
        let imports_good = import_requirements_satisfied(
            &symbols,
            &[
                "c_read",
                "c_write",
                "c_canonical_address",
                "c_human_address",
                "future_function",
            ],
        );
        assert_eq!(imports_good, true);
    }

    #[test]
    fn test_required_exports() {
        let mut reader = std::io::Cursor::new(CONTRACT_0_6);
        let symbols = wasm_nm::symbols(PUBLIC_SYMBOLS.clone(), &mut reader).unwrap();

        // subset okay
        let exports_good = has_all_exports(&symbols, &["init", "handle", "allocate"]);
        assert_eq!(exports_good, true);

        // match okay
        let exports_good = has_all_exports(
            &symbols,
            &[
                "query",
                "init",
                "handle",
                "allocate",
                "deallocate",
                "cosmwasm_api_0_6",
            ],
        );
        assert_eq!(exports_good, true);

        // missing one from list not okay
        let exports_good = has_all_exports(&symbols, &["init", "handle", "extra"]);
        assert_eq!(exports_good, false);
    }

    #[test]
    fn test_api_compatibility() {
        use crate::errors::Error;
        use wabt::wat2wasm;

        // this is our reference check, must pass
        check_api_compatibility(CONTRACT_0_7).unwrap();

        // Old 0.6 contract rejected since it requires outdated imports `c_read` and friends
        match check_api_compatibility(CONTRACT_0_6) {
            Err(Error::ValidationErr { msg }) => {
                assert_eq!(msg, EXTRA_IMPORT_MSG);
            }
            Err(e) => panic!("Unexpected error {:?}", e),
            Ok(_) => panic!("Didn't reject wasm with invalid api"),
        }

        // this is invalid, as it doesn't contain all required exports
        static WAT_MISSING_EXPORTS: &'static str = r#"
            (module
              (type $t0 (func (param i32) (result i32)))
              (func $add_one (export "add_one") (type $t0) (param $p0 i32) (result i32)
                get_local $p0
                i32.const 1
                i32.add))
        "#;

        let wasm_missing_exports = wat2wasm(WAT_MISSING_EXPORTS).unwrap();

        match check_api_compatibility(&wasm_missing_exports) {
            Err(Error::ValidationErr { msg }) => {
                assert_eq!(msg, MISSING_EXPORT_MSG);
            }
            Err(e) => panic!("Unexpected error {:?}", e),
            Ok(_) => panic!("Didn't reject wasm with invalid api"),
        }
    }
}