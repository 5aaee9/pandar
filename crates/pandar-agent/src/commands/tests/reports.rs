use serde::Serialize;

pub(super) fn get_version_report(model: &str) -> serde_json::Value {
    value(TestGetVersionReport {
        info: TestGetVersionInfo {
            command: "get_version",
            module: [TestGetVersionModule {
                name: "ota",
                product_name: model,
            }],
        },
    })
}

pub(super) fn ams_ready_report(material_type: &str) -> serde_json::Value {
    value(TestPrintReport {
        print: TestPrintReportPayload {
            gcode_state: "READY",
            ams: TestAmsReport {
                ams: [TestAmsUnitReport {
                    id: "0",
                    tray: [TestAmsTrayReport {
                        id: "0",
                        tray_type: material_type,
                    }],
                }],
            },
        },
    })
}

#[derive(Debug, Serialize)]
struct TestGetVersionReport<'a> {
    info: TestGetVersionInfo<'a>,
}

#[derive(Debug, Serialize)]
struct TestGetVersionInfo<'a> {
    command: &'static str,
    module: [TestGetVersionModule<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestGetVersionModule<'a> {
    name: &'static str,
    product_name: &'a str,
}

#[derive(Debug, Serialize)]
struct TestPrintReport<'a> {
    print: TestPrintReportPayload<'a>,
}

#[derive(Debug, Serialize)]
struct TestPrintReportPayload<'a> {
    gcode_state: &'static str,
    ams: TestAmsReport<'a>,
}

#[derive(Debug, Serialize)]
struct TestAmsReport<'a> {
    ams: [TestAmsUnitReport<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestAmsUnitReport<'a> {
    id: &'static str,
    tray: [TestAmsTrayReport<'a>; 1],
}

#[derive(Debug, Serialize)]
struct TestAmsTrayReport<'a> {
    id: &'static str,
    tray_type: &'a str,
}

fn value(input: impl Serialize) -> serde_json::Value {
    serde_json::to_value(input).unwrap()
}
