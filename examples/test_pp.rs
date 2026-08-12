use rosu_pp::{Difficulty, Beatmap};
use rosu_pp::osu::OsuPerformance;

fn main() {
    let maps = vec![
        ("resources/4848667.osu", "osu! standard map"),
    ];

    let mod_sets: Vec<(u32, &str)> = vec![
        // (0,      "NM"),
        (2,      "EZ"),
        // (8,      "HD"),
        // (16,     "HR"),
        // (8+16,   "HDHR"),
        // (64,     "DT"),
        // (8+64,   "HDDT"),
        // (16+64,  "HRDT"),
        // (8+16+64,"HDHRDT"),
        // (1024,   "FL"),
        // (8+1024, "HDFL"),
        // (1 << 7,"RX"),
        // (8+(1 << 7), "HDRX"),
        // (16+(1 << 7), "HRRX"),
        // (8+16+(1 << 7), "HDRXHR"),
        // (64+(1 << 7), "DTRX"),
        // (8+64+(1 << 7), "HDDTRX"),
        // (16+64+(1 << 7), "HRDTRX"),
        // (8+16+64+(1 << 7), "HDHRDTRX"),
    ];

    for (path, desc) in &maps {
        println!("Map: {} ({})", path, desc);
        println!("{}", "-".repeat(95));

        let map = match Beatmap::from_path(path) {
            Ok(m) => m,
            Err(e) => {
                println!("  Failed to decode: {}\n", e);
                continue;
            }
        };

        for &(mods, mod_name) in &mod_sets {
            let result = Difficulty::new()
                .mods(mods)
                .calculate(&map);

            let stars = result.stars();

            let perf = match OsuPerformance::from(&map)
                .mods(mods)
                .calculate()
            {
                Ok(p) => p,
                Err(e) => {
                    println!("  {:>8} | Stars: {:6.2} | PP calc error: {}", mod_name, stars, e);
                    continue;
                }
            };

            println!(
                "  {:>8} | Stars: {:6.2} | PP: {:8.2} | Aim: {:6.2} | Speed: {:6.2} | Acc: {:6.2} | FL: {:6.2}",
                mod_name, stars, perf.pp(), perf.pp_aim, perf.pp_speed, perf.pp_acc, perf.pp_flashlight
            );
            println!(
                "           | Aim: {:6.2} | Speed: {:6.2} | FL: {:6.2} | Reading: {:6.2}",
                perf.difficulty.aim, perf.difficulty.speed, perf.difficulty.flashlight, perf.difficulty.reading
            );
        }
        println!();
    }
}
