#pragma once

#include <cstdint>
#include <string>

#include <nlohmann/json.hpp>

#include "pinned_consumer_hashes.hpp"

namespace studio_print_consumer {

inline bool consume_task_page(const std::string& body)
{
    try {
        const auto page = nlohmann::json::parse(body);
        const int total = page.at("total").get<int>();
        const auto& hits = page.at("hits");
        if (!hits.is_array() || total != static_cast<int>(hits.size())) return false;
        for (const auto& hit : hits) {
            const std::int64_t design_id = hit.at("designId").get<std::int64_t>();
            const std::string title = design_id > 0
                ? hit.at("designTitle").get<std::string>()
                : hit.at("title").get<std::string>();
            const std::string device_name = hit.at("deviceName").get<std::string>();
            const std::string device_id = hit.at("deviceId").get<std::string>();
            const std::int64_t id = hit.at("id").get<std::int64_t>();
            const int status = hit.at("status").get<int>();
            const std::string cover = hit.at("cover").get<std::string>();
            const std::string start_time = hit.at("startTime").get<std::string>();
            const std::string end_time = hit.at("endTime").get<std::string>();
            const std::int64_t profile_id = hit.at("profileId").get<std::int64_t>();
            if (id <= 0 || profile_id <= 0 || status <= 0 || title.empty() ||
                device_name.empty() || device_id.empty() || start_time.empty()) {
                return false;
            }
            (void)cover;
            (void)end_time;
        }
        return true;
    } catch (...) {
        return false;
    }
}

inline bool consume_subtask(const std::string& body)
{
    try {
        const auto task = nlohmann::json::parse(body);
        const std::string content = task.at("content").get<std::string>();
        const auto info = nlohmann::json::parse(content);
        const int plate_index = info.at("info").at("plate_idx").get<int>();
        const auto& plates = task.at("context").at("plates");
        if (!plates.is_array()) return false;
        for (const auto& plate : plates) {
            if (plate.at("index").get<int>() != plate_index) continue;
            const int prediction = plate.at("prediction").get<int>();
            const float weight = plate.at("weight").get<float>();
            if (plate.contains("thumbnail")) {
                (void)plate.at("thumbnail").at("url").get<std::string>();
            }
            const auto& filaments = plate.at("filaments");
            if (!filaments.is_array()) return false;
            for (const auto& filament : filaments) {
                const std::string color = filament.at("color").get<std::string>();
                const std::string type = filament.at("type").get<std::string>();
                const float used_g = std::stof(filament.at("used_g").get<std::string>());
                const float used_m = std::stof(filament.at("used_m").get<std::string>());
                if (color.empty() || type.empty() || used_g < 0.0F || used_m < 0.0F) return false;
            }
            return prediction >= 0 && weight >= 0.0F;
        }
        return false;
    } catch (...) {
        return false;
    }
}

} // namespace studio_print_consumer
