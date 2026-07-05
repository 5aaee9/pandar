#pragma once

std::uint64_t parse_u64_or_zero(const std::string& value) {
    if (value.empty()) return 0;
    try {
        return static_cast<std::uint64_t>(std::stoull(value));
    } catch (...) {
        return 0;
    }
}

std::string hex_string(std::uint64_t value) {
    std::ostringstream out;
    out << std::hex << value;
    return out.str();
}

bool is_json_number(const std::string& value) {
    if (value.empty()) return false;
    std::size_t start = value[0] == '-' || value[0] == '+' ? 1 : 0;
    if (start == value.size()) return false;
    bool seen_digit = false;
    bool seen_dot = false;
    for (std::size_t i = start; i < value.size(); ++i) {
        const char c = value[i];
        if (c >= '0' && c <= '9') {
            seen_digit = true;
        } else if (c == '.' && !seen_dot) {
            seen_dot = true;
        } else {
            return false;
        }
    }
    return seen_digit;
}

std::string json_scalar_or_string(const std::string& value) {
    if (is_json_number(value)) return value;
    return escape_json(value);
}

std::string studio_tray_json(const std::string& tray) {
    const auto tray_id = scalar_from_json(tray, "tray_id");
    if (tray_id.empty()) return {};
    std::string out = std::string(R"({"id":)") + escape_json(tray_id);
    if (const auto value = scalar_from_json(tray, "filament_id"); !value.empty()) {
        out += R"(,"tray_info_idx":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(tray, "type"); !value.empty()) {
        out += R"(,"tray_type":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(tray, "color"); !value.empty()) {
        out += R"(,"tray_color":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(tray, "k_value"); !value.empty()) {
        out += R"(,"k":)" + json_scalar_or_string(value);
    }
    if (const auto value = scalar_from_json(tray, "remaining_estimate"); !value.empty()) {
        out += R"(,"remain":)" + json_scalar_or_string(value);
    }
    out += "}";
    return out;
}

std::string studio_ams_unit_json(
    const std::string& unit,
    std::uint64_t& ams_exist_bits,
    std::uint64_t& tray_exist_bits
) {
    const auto unit_id = scalar_from_json(unit, "unit_id");
    if (unit_id.empty()) return {};
    const auto unit_number = parse_u64_or_zero(unit_id);
    if (unit_number < 64) ams_exist_bits |= (std::uint64_t{1} << unit_number);

    const auto toolhead = scalar_from_json(unit, "toolhead");
    const auto extruder_id = toolhead == "L" || toolhead == "l" ? 1 : 0;
    const auto info = std::to_string(1 | (extruder_id << 8));
    std::string out = std::string(R"({"id":)") + escape_json(unit_id) +
        R"(,"info":)" + escape_json(info);
    if (const auto value = scalar_from_json(unit, "humidity_level"); !value.empty()) {
        out += R"(,"humidity":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(unit, "humidity"); !value.empty()) {
        out += R"(,"humidity_raw":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(unit, "temperature_celsius"); !value.empty()) {
        out += R"(,"temp":)" + escape_json(value);
    }
    out += R"(,"tray":[)";
    bool first = true;
    for (const auto& tray : objects_from_array(unit, "trays")) {
        const auto tray_json = studio_tray_json(tray);
        if (tray_json.empty()) continue;
        if (!first) out += ",";
        out += tray_json;
        first = false;
        if (const auto global = scalar_from_json(tray, "global_tray_id"); !global.empty()) {
            const auto global_number = parse_u64_or_zero(global);
            if (global_number < 64) tray_exist_bits |= (std::uint64_t{1} << global_number);
        } else {
            const auto global_number = unit_number * 4 + parse_u64_or_zero(scalar_from_json(tray, "tray_id"));
            if (global_number < 64) tray_exist_bits |= (std::uint64_t{1} << global_number);
        }
    }
    out += "]}";
    return out;
}

std::string studio_virtual_slot_json(const std::string& spool, std::size_t index) {
    std::string id = scalar_from_json(spool, "external_id");
    if (id != "254" && id != "255") {
        const auto toolhead = scalar_from_json(spool, "toolhead");
        id = toolhead == "L" || toolhead == "l" ? "254" : index == 0 ? "255" : "254";
    }
    std::string out = std::string(R"({"id":)") + escape_json(id);
    if (const auto value = scalar_from_json(spool, "filament_id"); !value.empty()) {
        out += R"(,"tray_info_idx":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(spool, "type"); !value.empty()) {
        out += R"(,"tray_type":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(spool, "color"); !value.empty()) {
        out += R"(,"tray_color":)" + escape_json(value);
    }
    if (const auto value = scalar_from_json(spool, "k_value"); !value.empty()) {
        out += R"(,"k":)" + json_scalar_or_string(value);
    }
    if (const auto value = scalar_from_json(spool, "remaining_estimate"); !value.empty()) {
        out += R"(,"remain":)" + json_scalar_or_string(value);
    }
    out += "}";
    return out;
}

std::string studio_tray_now_json(const std::string& materials) {
    const auto active = object_from_json(materials, "active_tray");
    if (active.empty()) return {};
    if (const auto global = scalar_from_json(active, "global_tray_id"); !global.empty()) {
        return R"(,"tray_now":)" + escape_json(global);
    }
    if (scalar_from_json(active, "kind") == "external") {
        const auto external_id = scalar_from_json(active, "external_id");
        return R"(,"tray_now":)" + escape_json(external_id.empty() ? "255" : external_id);
    }
    const auto ams_id = parse_u64_or_zero(scalar_from_json(active, "ams_id"));
    const auto tray_id = parse_u64_or_zero(scalar_from_json(active, "tray_id"));
    return R"(,"tray_now":)" + escape_json(std::to_string(ams_id * 4 + tray_id));
}

std::string studio_materials_payload(const std::string& printer) {
    const auto materials = object_from_json(printer, "materials");
    if (materials.empty()) return R"(,"ams":{"ams":[]})";

    std::uint64_t ams_exist_bits = 0;
    std::uint64_t tray_exist_bits = 0;
    std::string ams_units;
    bool first = true;
    for (const auto& unit : objects_from_array(materials, "ams_units")) {
        const auto unit_json = studio_ams_unit_json(unit, ams_exist_bits, tray_exist_bits);
        if (unit_json.empty()) continue;
        if (!first) ams_units += ",";
        ams_units += unit_json;
        first = false;
    }

    std::string out = std::string(R"(,"ams":{"ams":[)") + ams_units +
        R"(],"ams_exist_bits":)" + escape_json(hex_string(ams_exist_bits)) +
        R"(,"tray_exist_bits":)" + escape_json(hex_string(tray_exist_bits)) +
        studio_tray_now_json(materials) + "}";

    const auto external_spools = objects_from_array(materials, "external_spools");
    if (!external_spools.empty()) {
        out += R"(,"vir_slot":[)";
        for (std::size_t i = 0; i < external_spools.size(); ++i) {
            if (i != 0) out += ",";
            out += studio_virtual_slot_json(external_spools[i], i);
        }
        out += "]";
    }
    return out;
}
