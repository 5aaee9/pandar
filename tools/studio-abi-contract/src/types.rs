use std::{collections::BTreeSet, fs, path::Path};

use crate::source::StudioContract;

pub fn verify_pandar_abi_contract(contract: &StudioContract) -> Result<(), String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "studio-abi-contract must remain under tools/".to_owned())?;
    let shim_path = repo_root.join("crates/pandar-network-plugin/src/shim_types.hpp");
    let shim = fs::read_to_string(&shim_path)
        .map_err(|error| format!("read Pandar ABI types {}: {error}", shim_path.display()))?;
    verify_fields(
        "PrintParams",
        &contract.print_params_fields,
        &cpp_struct_fields(&shim, "PrintParams")?,
    )?;
    verify_fields(
        "AmsSyncItem",
        &contract.ams_sync_item_fields,
        &cpp_struct_fields(&shim, "AmsSyncItem")?,
    )?;
    verify_fields(
        "AmsSyncParams",
        &contract.ams_sync_params_fields,
        &cpp_struct_fields(&shim, "AmsSyncParams")?,
    )?;

    let exports_path = repo_root.join("crates/pandar-network-plugin/src/shim_exports.hpp");
    let exports = fs::read_to_string(&exports_path).map_err(|error| {
        format!(
            "read Pandar Studio export map {}: {error}",
            exports_path.display()
        )
    })?;
    let declarations = studio_export_map(&exports)?;
    let network = declarations
        .iter()
        .filter(|(symbol, _)| symbol.starts_with("bambu_network_"))
        .cloned()
        .collect::<Vec<_>>();
    let file_transfer = declarations
        .iter()
        .filter(|(symbol, _)| symbol.starts_with("ft_"))
        .cloned()
        .collect::<Vec<_>>();
    if network.len() != 109 || file_transfer.len() != 21 || declarations.len() != 130 {
        return Err(format!(
            "Pandar Studio export map must contain exactly 109 network and 21 FT records, got {} network, {} FT, {} total",
            network.len(),
            file_transfer.len(),
            declarations.len()
        ));
    }
    verify_export_map("network", &contract.network_exports, &network)?;
    verify_export_map("FT", &contract.file_transfer_exports, &file_transfer)?;
    let upstream = contract
        .network_exports
        .iter()
        .chain(&contract.file_transfer_exports)
        .cloned()
        .collect::<Vec<_>>();
    verify_export_map("combined", &upstream, &declarations)
}

fn verify_fields(name: &str, upstream: &[String], pandar: &[String]) -> Result<(), String> {
    if pandar == upstream {
        return Ok(());
    }
    let mismatch = upstream
        .iter()
        .zip(pandar)
        .position(|(upstream, pandar)| upstream != pandar)
        .unwrap_or_else(|| upstream.len().min(pandar.len()));
    Err(format!(
        "Pandar {name} field order differs at index {mismatch}: upstream={:?}, pandar={:?}",
        upstream.get(mismatch),
        pandar.get(mismatch)
    ))
}

fn verify_export_map(
    family: &str,
    upstream: &[(String, String)],
    pandar: &[(String, String)],
) -> Result<(), String> {
    if pandar == upstream {
        return Ok(());
    }
    let mismatch = upstream
        .iter()
        .zip(pandar)
        .position(|(upstream, pandar)| upstream != pandar)
        .unwrap_or_else(|| upstream.len().min(pandar.len()));
    Err(format!(
        "Pandar {family} symbol-to-typedef map differs at index {mismatch}: upstream={:?}, pandar={:?}",
        upstream.get(mismatch),
        pandar.get(mismatch)
    ))
}

pub fn cpp_struct_fields(contents: &str, name: &str) -> Result<Vec<String>, String> {
    let contents = strip_comments(contents);
    let marker = format!("struct {name}");
    let declaration = contents
        .find(&marker)
        .ok_or_else(|| format!("missing {marker} declaration"))?;
    let open = contents[declaration..]
        .find('{')
        .map(|offset| declaration + offset)
        .ok_or_else(|| format!("missing opening brace for {marker}"))?;
    let close = matching_brace(&contents, open)
        .ok_or_else(|| format!("missing closing brace for {marker}"))?;
    let fields = contents[open + 1..close]
        .split(';')
        .filter_map(field_name)
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return Err(format!("{marker} contains no fields"));
    }
    Ok(fields)
}

pub fn studio_export_map(contents: &str) -> Result<Vec<(String, String)>, String> {
    let mut exports = Vec::new();
    let mut symbols = BTreeSet::new();
    for (index, line) in contents.lines().enumerate() {
        let Some(entry) = line.trim().strip_prefix("PANDAR_STUDIO_EXPORT(") else {
            continue;
        };
        let mut fields = entry.splitn(3, ',').map(str::trim);
        let symbol = fields
            .next()
            .filter(|symbol| symbol.starts_with("bambu_network_") || symbol.starts_with("ft_"))
            .ok_or_else(|| format!("unparseable Studio export symbol at line {}", index + 1))?;
        let target = fields
            .next()
            .filter(|target| !target.is_empty())
            .ok_or_else(|| format!("unparseable Studio export typedef at line {}", index + 1))?;
        if fields.next().is_none() {
            return Err(format!(
                "incomplete Studio export record at line {}",
                index + 1
            ));
        }
        if !symbols.insert(symbol.to_owned()) {
            return Err(format!("duplicate Studio export symbol {symbol}"));
        }
        exports.push((symbol.to_owned(), target.to_owned()));
    }
    Ok(exports)
}

fn matching_brace(contents: &str, open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for (offset, character) in contents[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn field_name(declaration: &str) -> Option<String> {
    let declaration = declaration.trim();
    if declaration.is_empty() {
        return None;
    }
    let initializer = declaration
        .find('=')
        .into_iter()
        .chain(declaration.find('{'))
        .min()
        .unwrap_or(declaration.len());
    declaration[..initializer]
        .split_whitespace()
        .last()
        .map(|name| name.trim_start_matches(['*', '&']).to_owned())
}

fn strip_comments(contents: &str) -> String {
    let mut output = String::with_capacity(contents.len());
    let mut characters = contents.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '/' {
            output.push(character);
            continue;
        }
        match characters.peek() {
            Some('/') => {
                characters.next();
                for next in characters.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            Some('*') => {
                characters.next();
                let mut previous = '\0';
                for next in characters.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                output.push(' ');
            }
            _ => output.push(character),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{cpp_struct_fields, studio_export_map};

    #[test]
    fn extracts_field_order_across_comments_and_initializers() {
        let source = r#"
            struct PrintParams {
                std::string dev_id;
                int plate_index = 0; // comment
                bool enabled{ false };
                /* tail */ std::string slicer_uid;
            };
        "#;

        assert_eq!(
            cpp_struct_fields(source, "PrintParams").unwrap(),
            ["dev_id", "plate_index", "enabled", "slicer_uid"]
        );
    }

    #[test]
    fn extracts_reviewed_symbol_to_typedef_map() {
        let source = r#"
            #define PANDAR_STUDIO_EXPORT(name, target, result, parameters)
            PANDAR_STUDIO_EXPORT(bambu_network_bind, func_bind, int, (void*, bool))
            PANDAR_STUDIO_EXPORT(ft_abi_version, fn_ft_abi_version, int, ())
        "#;

        assert_eq!(
            studio_export_map(source).unwrap(),
            [
                ("bambu_network_bind".to_owned(), "func_bind".to_owned()),
                ("ft_abi_version".to_owned(), "fn_ft_abi_version".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_duplicate_or_incomplete_reviewed_exports() {
        let duplicate = r#"
            PANDAR_STUDIO_EXPORT(ft_free, fn_ft_free, void, (void*))
            PANDAR_STUDIO_EXPORT(ft_free, fn_ft_free, void, (void*))
        "#;
        assert!(
            studio_export_map(duplicate)
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(
            studio_export_map("PANDAR_STUDIO_EXPORT(ft_free, fn_ft_free)")
                .unwrap_err()
                .contains("incomplete")
        );
    }
}
