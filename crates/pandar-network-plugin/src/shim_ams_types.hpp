#pragma once

#include <string>
#include <vector>

#if defined(PANDAR_STUDIO_AMS_SYNC)
namespace BBL {

struct AmsSyncItem {
    std::string RFID;
    std::string filamentVendor;
    std::string filamentType;
    std::string filamentName;
    std::string filamentId;
    bool isSupport = false;
    std::string color;
    int colorType = 0;
    std::vector<std::string> colors;
    int netWeight = 0;
    int totalNetWeight = 0;
    std::string trayIdName;
    std::string note;
    std::string amsSn;
    std::string slotId;
    int amsId = 0;
    int amsType = 0;
    bool createNew = false;
};

struct AmsSyncParams {
    std::string devId;
    std::vector<AmsSyncItem> items;
};

} // namespace BBL
#endif
