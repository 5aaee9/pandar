#pragma once

#include <string>
#include <vector>

#if defined(PANDAR_STUDIO_SLOT_MAPPINGS_SYNC)
namespace BBL {

struct SlotMappingItem {
    std::string amsSn;
    std::string slotId;
    int spoolId = 0;
    std::string rfid;
    int amsId = 0;
    int amsType = 0;
};

struct SlotMappingsSyncParams {
    std::string devId;
    std::vector<SlotMappingItem> mappings;
};

} // namespace BBL
#endif
