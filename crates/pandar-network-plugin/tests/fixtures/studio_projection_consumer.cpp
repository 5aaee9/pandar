#include <cassert>
#include <cstdint>
#include <ctime>
#include <iostream>
#include <limits>
#include <optional>
#include <string>

#include <nlohmann/json.hpp>

#define BOOST_LOG_TRIVIAL(level) if (false) std::cerr
#define HOLD_TIME_3SEC 3

using namespace nlohmann;

namespace Slic3r {

class MachineObject;

class NozzleSystemProbe {
public:
    void SetSupportNozzleRack(bool supported) { m_supported = supported; }
    bool supported() const { return m_supported; }

private:
    bool m_supported{false};
};

class DevStorage {
public:
    enum SdcardState : int {
        NO_SDCARD = 0,
        HAS_SDCARD_NORMAL = 1,
        HAS_SDCARD_ABNORMAL = 2,
        HAS_SDCARD_READONLY = 3,
        SDCARD_STATE_NUM = 4,
    };

    SdcardState set_sdcard_state(int state);
    SdcardState get_sdcard_state() const { return m_sdcard_state; }

private:
    SdcardState m_sdcard_state{NO_SDCARD};
};

class MachineObject {
public:
    bool check_enable_np(const json& print) const;
    int get_flag_bits(std::string str, int start, int count = 1) const;

    void parse_camera(json print)
    {
        std::string cfg = print["cfg"].get<std::string>();
        if (!cfg.empty()) {
#include "device_cfg_camera.4352-4353.cpp"
        }
        std::string fun = print["fun"].get<std::string>();
        if (!fun.empty()) {
#include "device_fun_agora.4361-4362.cpp"
#include "device_fun_camera.4373-4375.cpp"
        }
    }

    void parse_aux(json print)
    {
#include "device_aux.4417-4425.cpp"
    }

    void parse_wtm(json print)
    {
        std::string fun = print["fun"].get<std::string>();
        if (!fun.empty()) {
#include "device_fun_wtm.4381.cpp"
        }
    }

    void parse_ext_change_assist(json print)
    {
        std::string fun = print["fun"].get<std::string>();
        if (!fun.empty()) {
#include "device_fun_ext_change_assist.4378.cpp"
        }
    }

    int sdcard_state() const { return m_storage->get_sdcard_state(); }
    bool camera_hidden() const
    {
        return !is_support_agora && !is_support_brtc && !is_support_liveview_preview;
    }
    bool nozzle_rack_supported() const { return nozzle_system.supported(); }
    bool ext_change_assist_supported() const { return is_support_ext_change_assist; }

private:
    DevStorage storage;
    DevStorage* m_storage{&storage};
    bool installed_upgrade_kit{false};
    bool is_support_liveview_preview{false};
    bool is_support_agora{false};
    bool is_support_tunnel_mqtt{true};
    bool is_support_internal_timelapse{false};
    bool is_support_brtc{false};
    bool m_support_mqtt_bet_ctrl{false};
    bool m_has_timelapse_kit{false};
    bool is_support_ext_change_assist{false};
    NozzleSystemProbe nozzle_system;
    NozzleSystemProbe* m_nozzle_system{&nozzle_system};
};

#include "device_check_enable.4265-4273.cpp"
#include "device_flag_bits.4458-4469.cpp"
#include "dev_storage.7-16.cpp"

class DevUtil {
public:
    static int get_flag_bits(std::string value, int start, int count = 1)
    {
        return MachineObject().get_flag_bits(std::move(value), start, count);
    }
};

class DevJsonValParser {
public:
    template<typename T>
    static T GetVal(const json& value, const std::string& key, const T& fallback = T())
    {
        return value.contains(key) ? value[key].get<T>() : fallback;
    }

    template<typename T>
    static void ParseVal(const json& value, const std::string& key, T& target)
    {
        if (value.contains(key)) target = value[key].get<T>();
    }
};

class DevInfo {
public:
    void ParseInfo(const json& print_jj);
    const std::string& connection_type() const { return m_dev_connection_type; }

private:
    std::string m_dev_connection_type;
    unsigned int m_device_mode{1};
};

#include "dev_info.29-36.cpp"

class DevConfig {
public:
    explicit DevConfig(MachineObject*) {}
    bool HasChamber() const { return m_has_chamber; }
    bool SupportChamberTempDisplay() const;
    void ParseConfig(const json& print_json);
    void ParseChamberConfig(const json& print_json);
    void ParsePrintOptionsConfig(const json& print_json);
    void ParseCalibrationConfig(const json& print_json);

private:
    bool m_has_chamber{false};
    std::optional<bool> m_support_chamber_temp_display;
    bool m_support_chamber_edit{false};
    int m_chamber_temp_edit_min{0};
    int m_chamber_temp_edit_max{60};
    int m_chamber_temp_switch_heat{std::numeric_limits<int>::max()};
    bool m_support_first_layer_inspect{false};
    bool m_support_save_remote_print_file_to_storage{false};
    bool m_support_ai_monitor{false};
    bool m_support_print_without_sd{false};
    bool m_support_print_all{false};
    bool m_support_calibration_lidar{false};
    bool m_support_calibration_nozzle_offset{false};
    bool m_support_calibration_high_temp_bed{false};
    bool m_support_calibration_pa_flow_auto{false};
    bool m_support_calibration_clump_pos{false};
};

#include "dev_config.11-67.cpp"

class DevAxis {
public:
    void ParseAxis(const json& print_json);
    bool supports_homing() const { return m_is_support_mqtt_homing; }
    bool supports_axis_control() const { return m_is_support_mqtt_axis_ctrl; }

private:
    int m_home_flag{0};
    bool m_is_support_mqtt_axis_ctrl{false};
    bool m_is_support_mqtt_homing{false};
};

#include "dev_axis.9-18.cpp"

struct Detection {
    std::time_t detect_hold_start{-100};
    int current_detect_value{0};
    bool is_support_detect{false};
};

struct PrintOptions {
    Detection m_snapshot_detection;
    Detection m_filament_tangle_detection;
    Detection m_spaghetti_detection;
    Detection m_purgechutepileup_detection;
    Detection m_nozzleclumping_detection;
    Detection m_airprinting_detection;
    Detection m_idel_heating_protect_detection;
    Detection m_allow_prompt_sound_detection;
    Detection m_nozzle_blob_detection;
};

void parse_detection_options(const json& print_json, PrintOptions* opts)
{
    std::string cfg = print_json.value("cfg", "");
    if (!cfg.empty()) {
#include "dev_options_cfg.227-231.cpp"
    }
#include "dev_options_fun.234-247.cpp"
}

} // namespace Slic3r

int main(int argc, char** argv)
{
    if (argc != 2) return 2;
    const auto print = json::parse(argv[1]);
    Slic3r::MachineObject machine;
    const bool gate = machine.check_enable_np(print);
    if (gate) {
        machine.parse_camera(print);
        machine.parse_aux(print);
        machine.parse_wtm(print);
        machine.parse_ext_change_assist(print);
    }
    Slic3r::DevConfig chamber(&machine);
    chamber.ParseChamberConfig(print);
    Slic3r::DevAxis axis;
    axis.ParseAxis(print);
    Slic3r::PrintOptions options;
    Slic3r::parse_detection_options(print, &options);
    Slic3r::DevInfo info;
    info.ParseInfo(print);
    const bool unsupported_fun_hidden =
        !options.m_filament_tangle_detection.is_support_detect &&
        !options.m_spaghetti_detection.is_support_detect &&
        !options.m_purgechutepileup_detection.is_support_detect &&
        !options.m_nozzleclumping_detection.is_support_detect &&
        !options.m_airprinting_detection.is_support_detect &&
        !options.m_idel_heating_protect_detection.is_support_detect &&
        !options.m_allow_prompt_sound_detection.is_support_detect &&
        !options.m_nozzle_blob_detection.is_support_detect;
    json output = {
        {"gate", gate},
        {"sdcard_state", machine.sdcard_state()},
        {"camera_hidden", machine.camera_hidden()},
        {"chamber", chamber.HasChamber()},
        {"chamber_display", chamber.SupportChamberTempDisplay()},
        {"axis_homing", axis.supports_homing()},
        {"axis_control", axis.supports_axis_control()},
        {"snapshot_detection", options.m_snapshot_detection.is_support_detect},
        {"unsupported_fun_hidden", unsupported_fun_hidden},
        {"nozzle_rack_supported", machine.nozzle_rack_supported()},
        {"ext_change_assist_supported", machine.ext_change_assist_supported()},
        {"connection_type", info.connection_type()},
    };
    std::cout << output.dump() << '\n';
    return 0;
}
