use clap::Parser;
use osu_db::collection::Collection;
use osu_db::listing::Beatmap;
use osu_db::{CollectionList, Listing, Mode};
use rosu_pp::{BeatmapExt, PerformanceAttributes};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
/// Create osu! collections based on aim/tapping ratio
struct Args {
    #[arg(default_value = ".")]
    /// Path to the osu! directory
    osu_path: String,

    #[arg(long, default_value = "% ")]
    /// The prefix to add to each collection
    collection_prefix: String,

    #[arg(long, default_value = "10.0")]
    /// The multiples of which the aim ratio is grouped by (eg. precision 5 => groups of 50%, 55%, 60%...)
    ratio_precision: f64,

    #[arg(long, default_value = "4.0")]
    /// The minimum star rating to consider for collections (will speed up the process a lot)
    min_star_rating: f64,
}

fn main() {
    let args = Args::parse();

    println!("Starting process with following arguments: {:?}", args);

    let osu_path = Path::new(&args.osu_path);

    let db_path = osu_path.join("osu!.db");
    if !db_path.exists() {
        panic!("osu!.db was not found");
    }

    let collection_path = osu_path.join("collection.db");
    if !collection_path.exists() {
        panic!("collection.db was not found");
    }

    println!("Reading osu!.db");
    let listing = Listing::from_file(&db_path).expect("Could not read osu!.db");
    println!("Finished reading osu!.db");

    let aim_ratio_groups = group_maps_by(&args, listing);

    println!("Reading collection.db");

    let mut collections = CollectionList::from_file(&collection_path).unwrap();

    remove_previous_collections(&args, &mut collections);
    add_new_collections(&args, aim_ratio_groups, &mut collections);

    collections.to_file(collection_path).unwrap();

    println!("Successfully wrote collection.db");
}

fn group_maps_by(args: &Args, listing: Listing) -> HashMap<i32, Vec<Option<String>>> {
    let filtered_maps: Vec<&Beatmap> = listing
        .beatmaps
        .iter()
        .filter(|map| {
            map.mode == Mode::Standard
                && map
                .std_ratings
                .iter()
                .find_map(|(mods, stars)| if mods.0 == 0 { Some(stars) } else { None })
                .unwrap_or(&args.min_star_rating) // When star rating calcs haven't run yet, the star rating will not be set.
                >= &args.min_star_rating
        })
        .collect();

    println!(
        "Found {} out of {} total maps to process",
        filtered_maps.len(),
        listing.beatmaps.len()
    );

    let now = Instant::now();

    let mut count = 0;

    filtered_maps
        .iter()
        .fold(HashMap::new(), |mut hash_map, map| {
            let map_path = Path::new(&args.osu_path)
                .join("Songs")
                .join(map.folder_name.as_ref().unwrap())
                .join(map.file_name.as_ref().unwrap());

            let map_pp = match rosu_pp::Beatmap::from_path(&map_path) {
                Ok(map) => map,
                Err(why) => {
                    println!(
                        "Error while parsing {}: {}",
                        map_path.to_str().unwrap_or_default(),
                        why
                    );
                    return hash_map;
                }
            };

            if let PerformanceAttributes::Osu(pp) = map_pp.pp().accuracy(99f64).calculate() {
                let aim_aspect = pp.pp_aim / (pp.pp_aim + pp.pp_speed);
                let rounded_aim_aspect = ((aim_aspect * 100f64 / args.ratio_precision).floor()
                    * args.ratio_precision) as i32;

                Vec::push(
                    hash_map
                        .entry(rounded_aim_aspect)
                        .or_default(),
                    map.hash.clone(),
                );
            }

            count += 1;

            if count % 100 == 0 {
                println!(
                    "Processed {}/{} maps in {:.1} seconds",
                    count,
                    filtered_maps.len(),
                    now.elapsed().as_secs_f32()
                );
            }

            hash_map
        })
}

fn add_new_collections(
    args: &Args,
    aim_ratio_groups: HashMap<i32, Vec<Option<String>>>,
    collections: &mut CollectionList,
) {
    for (aim_ratio, maps) in aim_ratio_groups {
        let prefix = &args.collection_prefix;
        let collection_name = format!("{prefix}{aim_ratio}% Aim / {}% Tapping", 100 - aim_ratio);

        println!("Adding {collection_name} with {} maps", maps.len());

        collections.collections.push(Collection {
            name: Some(collection_name),
            beatmap_hashes: maps,
        });
    }
}

fn remove_previous_collections(args: &Args, collections: &mut CollectionList) {
    let collection_count = collections.collections.len();
    collections.collections.retain(|c| {
        if let Some(name) = &c.name {
            !name.starts_with(&args.collection_prefix)
        } else {
            true
        }
    });

    println!(
        "Removed {} collections from previous iteration",
        collection_count - collections.collections.len()
    );
}
