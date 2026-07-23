use std::collections::BTreeSet;

pub type ExportMap = Vec<(String, String)>;

pub fn loaded_export_map(
    contents: &str,
    loader: &str,
    type_marker: &str,
    prefix: &str,
) -> Result<ExportMap, String> {
    let mut exports = Vec::new();
    let mut symbols = BTreeSet::new();
    for (index, line) in contents.lines().enumerate() {
        if !line.contains(loader) {
            continue;
        }
        let matching_symbols = line
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|value| value.starts_with(prefix))
            .collect::<Vec<_>>();
        if matching_symbols.is_empty() {
            continue;
        }
        if matching_symbols.len() != 1 {
            return Err(format!(
                "line {} contains multiple {prefix} loader symbols",
                index + 1
            ));
        }
        let target = line
            .split_once(type_marker)
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(target, _)| target.trim())
            .filter(|target| !target.is_empty())
            .ok_or_else(|| {
                format!(
                    "line {} loads {} without a parseable typedef",
                    index + 1,
                    matching_symbols[0]
                )
            })?;
        let symbol = matching_symbols[0];
        if !symbols.insert(symbol.to_owned()) {
            return Err(format!("duplicate loaded Studio symbol {symbol}"));
        }
        exports.push((symbol.to_owned(), target.to_owned()));
    }
    Ok(exports)
}

#[cfg(test)]
mod tests {
    use super::loaded_export_map;

    #[test]
    fn preserves_loader_order_and_typedefs() {
        let source = r#"
            a = reinterpret_cast<func_start>(get_network_function("bambu_network_start"));
            b = reinterpret_cast<func_bind>(get_network_function("bambu_network_bind"));
        "#;

        assert_eq!(
            loaded_export_map(
                source,
                "get_network_function",
                "reinterpret_cast<",
                "bambu_network_"
            )
            .unwrap(),
            [
                ("bambu_network_start".to_owned(), "func_start".to_owned()),
                ("bambu_network_bind".to_owned(), "func_bind".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_duplicate_or_unparseable_loader_records() {
        let duplicate = r#"
            sym_lookup<fn_ft_free>(module, "ft_free");
            sym_lookup<fn_ft_free>(module, "ft_free");
        "#;
        assert!(
            loaded_export_map(duplicate, "sym_lookup", "sym_lookup<", "ft_")
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(
            loaded_export_map(
                "get_network_function(\"bambu_network_start\");",
                "get_network_function",
                "reinterpret_cast<",
                "bambu_network_"
            )
            .unwrap_err()
            .contains("parseable typedef")
        );
    }
}
