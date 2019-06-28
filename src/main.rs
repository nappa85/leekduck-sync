use std::collections::HashMap;

use structopt::StructOpt;

use reqwest::get;

use regex::Regex;

use mysql::Conn;

const URL: &str = "https://leekduck.com/boss/";

/// Leekduck radiboss importer
#[derive(StructOpt, Debug)]
#[structopt(name = "leekduck-sync")]
struct Opt {
    /// MySQL connection URL
    // #[structopt(short = "v", long = "verbose")]
    mysql_url: String,

    /// Do not apply changes, only show them
    #[structopt(short = "p", long = "pretend")]
    pretend: bool,
}

fn main() {
    let opt = Opt::from_args();

    let mut conn = Conn::new(&opt.mysql_url).expect("Can't connect to MySQL");
    let names: Vec<String> = conn.query("SELECT name FROM pokemon_list")
        .expect("Can't retrieve Pokémon list")
        .into_iter()
        .filter(Result::is_ok)
        .map(|row| {
            // as_sql returns the quoted string
            let s = row.unwrap()[0].as_sql(true);
            (&s[1..(s.len() - 1)]).to_owned()
        })
        .collect();

    let text = get(URL).expect(&format!("Failed to contact \"{}\"", URL)).text().expect("Failed to read response text");
    let re = Regex::new(r#"(<li\s+class="[^"]*header-li[^"]*"><h2\s+class="[^"]*tier\-(\d+)[^"]*"|<p\s+class="[^"]*boss\-name[^"]*">([^<>]+)</p>)"#).unwrap();
    let mut tiers = HashMap::new();
    let mut current_tier = None;
    for cap in re.captures_iter(&text) {
        if let Some(tier) = cap.get(2) {
            current_tier = Some(tier.as_str());
        }
        else if let Some(name) = cap.get(3) {
            if let Some(tier) = current_tier {
                let entry = tiers.entry(tier).or_insert_with(|| Vec::new());
                // filter retrieved words with Pokémon names list to filter out forms
                let name = name.as_str();
                for pokemon in &names {
                    // it's the name that contains Pokémon's name, not vice-versa
                    if name.contains(pokemon) {
                        entry.push(pokemon.as_str());
                        break;
                    }
                }
            }
        }
    }

    if opt.pretend {
        println!("UPDATE pokemon_list SET raid = 0;");
        for (tier, pokemons) in &tiers {
            println!("UPDATE pokemon_list SET raid = {} WHERE name IN ('{}');", tier, pokemons.join("', '"));
        }
    }
    else {
        let mut transaction = conn.start_transaction(true, None, None).expect("Error starting transaction");
        transaction.query("UPDATE pokemon_list SET raid = 0").expect("Error executing query");
        for (tier, mut pokemons) in tiers.into_iter() {
            let mut stmt = transaction.prepare(format!("UPDATE pokemon_list SET raid = ? WHERE name IN ({})", &("?, ".repeat(pokemons.len()))[..(3 * pokemons.len() - 2)])).expect("Cannot prepare query");
            let mut args = Vec::new();
            args.push(tier);
            args.append(&mut pokemons);
            stmt.execute(args).expect("Error executing query");
        }
        transaction.commit().expect("Error committing transaction");
    }
}
