This crawler program uses the league of legends api to go through players match
histories and record the starting conditions of a set number of games, along with the
outcome of that game (e.g. who won).

You could use this to visualize how a match's starting conditions affect the outcome
or you could create an ML model to predict match outcomes based on the match :)

In the future you will be able to filter by league season and specify how many games you
want to collect data on.

# Usage

The usage is pretty simple.

1. Acquire a riot api key from the [riot dev portal](https://developer.riotgames.com)
1. Navigate to the root directory of the `lol-match-crawler` project repo.
1. Create a file `key.txt` and copy your riot api key into it
1. run `cargo build` to build the program and fetch dependencies
1. Run `cargo run` to run the crawler and collect match data

# Data Format

Data is output in csv format. For a specification of the different fields,
see the `fields.txt` file. For a sample output see `lol_data-sample.csv`

> NOTE: These files have not been committed to the repo yet, but they're coming soon.