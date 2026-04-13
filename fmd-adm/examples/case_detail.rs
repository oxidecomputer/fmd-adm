use fmd_adm::{FmdAdm, NvList, NvValue};

fn print_nvlist(nvl: &NvList, indent: usize) {
    let pad = "  ".repeat(indent);
    for (name, value) in nvl {
        match value {
            NvValue::NvList(inner) => {
                println!("{pad}{name} = (embedded nvlist)");
                print_nvlist(inner, indent + 1);
            }
            NvValue::NvListArray(arr) => {
                println!("{pad}{name} = (array of {} nvlists)", arr.len());
                for (i, inner) in arr.iter().enumerate() {
                    println!("{pad}  [{i}]:");
                    print_nvlist(inner, indent + 2);
                }
            }
            other => {
                println!("{pad}{name} = {other:?}");
            }
        }
    }
}

fn main() {
    let adm = FmdAdm::open().expect("failed to open fmd adm handle");
    let cases = adm.cases(None).expect("failed to list cases");

    if let Some(c) = cases.first() {
        println!("Case {} ({})\n", c.uuid, c.code);
        if let Some(event) = &c.event {
            print_nvlist(event, 0);
        } else {
            println!("  (no event data)");
        }
    } else {
        println!("No cases found.");
    }
}
