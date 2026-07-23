#pragma once

#include <string>

namespace Slic3r {

class BBLModelTask {
public:
    BBLModelTask();
    ~BBLModelTask() {}

    int job_id;
    int design_id;
    int profile_id;
    int instance_id;
    std::string task_id;
    std::string model_id;
    std::string model_name;
    std::string profile_name;
};

} // namespace Slic3r
