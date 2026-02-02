// Universal Oracle Data Sources
// Free APIs for EVERYTHING: crypto, sports, weather, news, Wikipedia, stocks, etc.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Universal data source catalog
#[derive(Debug, Clone)]
pub struct DataSourceCatalog {
    sources: HashMap<DataCategory, Vec<DataSourceConfig>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DataCategory {
    Cryptocurrency,
    Sports,
    Weather,
    News,
    Finance,
    Wikipedia,
    Government,
    Science,
    Entertainment,
    Gaming,
    Social,
    Health,
    Transportation,
    Food,
    RandomData,
    // New categories
    Fun,        // Jokes, quotes, facts
    Geography,  // Countries, cities, ISS location
    Language,   // Translation, dictionaries
    Books,      // Open Library, book data
    Animals,    // Cat/dog facts, pet data
    Internet,   // IP data, DNS, domains
    Art,        // Museum data, color palettes
    Math,       // Number facts, calculations
    Education,  // Universities, courses
    Blockchain, // Additional blockchain data
}

#[derive(Debug, Clone)]
pub struct DataSourceConfig {
    pub name: String,
    pub base_url: String,
    pub requires_api_key: bool,
    pub rate_limit_per_minute: u32,
    pub description: String,
}

impl DataSourceCatalog {
    pub fn new() -> Self {
        let mut sources: HashMap<DataCategory, Vec<DataSourceConfig>> = HashMap::new();

        // ============= CRYPTOCURRENCY =============
        sources.insert(
            DataCategory::Cryptocurrency,
            vec![
                DataSourceConfig {
                    name: "CoinGecko".to_string(),
                    base_url: "https://api.coingecko.com/api/v3".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 50,
                    description: "Free crypto prices, market data, charts".to_string(),
                },
                DataSourceConfig {
                    name: "Binance".to_string(),
                    base_url: "https://api.binance.com/api/v3".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1200,
                    description: "Real-time crypto prices from Binance".to_string(),
                },
                DataSourceConfig {
                    name: "Coinbase".to_string(),
                    base_url: "https://api.coinbase.com/v2".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 10,
                    description: "Coinbase spot prices".to_string(),
                },
                DataSourceConfig {
                    name: "CoinCap".to_string(),
                    base_url: "https://api.coincap.io/v2".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 200,
                    description: "Crypto market data and prices".to_string(),
                },
                DataSourceConfig {
                    name: "Kraken".to_string(),
                    base_url: "https://api.kraken.com/0/public".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 15,
                    description: "Kraken exchange prices".to_string(),
                },
            ],
        );

        // ============= SPORTS =============
        sources.insert(
            DataCategory::Sports,
            vec![
                DataSourceConfig {
                    name: "TheSportsDB".to_string(),
                    base_url: "https://www.thesportsdb.com/api/v1/json/3".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "Free sports scores, teams, leagues, players".to_string(),
                },
                DataSourceConfig {
                    name: "NBA API".to_string(),
                    base_url: "https://www.balldontlie.io/api/v1".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "Free NBA stats and scores".to_string(),
                },
                DataSourceConfig {
                    name: "Football-Data.org".to_string(),
                    base_url: "https://api.football-data.org/v4".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 10,
                    description: "Soccer/Football scores and standings (free tier)".to_string(),
                },
            ],
        );

        // ============= WEATHER =============
        sources.insert(
            DataCategory::Weather,
            vec![
                DataSourceConfig {
                    name: "Open-Meteo".to_string(),
                    base_url: "https://api.open-meteo.com/v1".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 10000,
                    description: "100% FREE weather API, no key needed".to_string(),
                },
                DataSourceConfig {
                    name: "OpenWeatherMap".to_string(),
                    base_url: "https://api.openweathermap.org/data/2.5".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 60,
                    description: "Weather data (free tier: 60 calls/min)".to_string(),
                },
                DataSourceConfig {
                    name: "WeatherAPI".to_string(),
                    base_url: "https://api.weatherapi.com/v1".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 60,
                    description: "Weather forecasts and current conditions".to_string(),
                },
            ],
        );

        // ============= NEWS =============
        sources.insert(
            DataCategory::News,
            vec![
                DataSourceConfig {
                    name: "NewsAPI".to_string(),
                    base_url: "https://newsapi.org/v2".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 100,
                    description: "News headlines from 80,000+ sources (free tier)".to_string(),
                },
                DataSourceConfig {
                    name: "Hacker News".to_string(),
                    base_url: "https://hacker-news.firebaseio.com/v0".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1000,
                    description: "Tech news from Hacker News".to_string(),
                },
                DataSourceConfig {
                    name: "Reddit".to_string(),
                    base_url: "https://www.reddit.com".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "Reddit posts and comments (JSON API)".to_string(),
                },
            ],
        );

        // ============= FINANCE/STOCKS =============
        sources.insert(
            DataCategory::Finance,
            vec![
                DataSourceConfig {
                    name: "Alpha Vantage".to_string(),
                    base_url: "https://www.alphavantage.co/query".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 5,
                    description: "Stock prices, forex, crypto (free tier: 5/min)".to_string(),
                },
                DataSourceConfig {
                    name: "Yahoo Finance".to_string(),
                    base_url: "https://query1.finance.yahoo.com/v8/finance".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 2000,
                    description: "Stock quotes and market data".to_string(),
                },
                DataSourceConfig {
                    name: "Polygon.io".to_string(),
                    base_url: "https://api.polygon.io/v2".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 5,
                    description: "Stock market data (free tier)".to_string(),
                },
            ],
        );

        // ============= WIKIPEDIA =============
        sources.insert(
            DataCategory::Wikipedia,
            vec![
                DataSourceConfig {
                    name: "Wikipedia API".to_string(),
                    base_url: "https://en.wikipedia.org/w/api.php".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 200,
                    description: "Wikipedia articles, edits, page views".to_string(),
                },
                DataSourceConfig {
                    name: "Wikidata".to_string(),
                    base_url: "https://www.wikidata.org/w/api.php".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 200,
                    description: "Structured knowledge base".to_string(),
                },
            ],
        );

        // ============= GOVERNMENT DATA =============
        sources.insert(
            DataCategory::Government,
            vec![
                DataSourceConfig {
                    name: "NASA APIs".to_string(),
                    base_url: "https://api.nasa.gov".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 1000,
                    description: "Space data, images, APOD (free key)".to_string(),
                },
                DataSourceConfig {
                    name: "USA.gov".to_string(),
                    base_url: "https://api.usa.gov".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1000,
                    description: "US government data".to_string(),
                },
                DataSourceConfig {
                    name: "World Bank".to_string(),
                    base_url: "https://api.worldbank.org/v2".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 120,
                    description: "Global economic and development data".to_string(),
                },
            ],
        );

        // ============= SCIENCE =============
        sources.insert(
            DataCategory::Science,
            vec![
                DataSourceConfig {
                    name: "arXiv".to_string(),
                    base_url: "http://export.arxiv.org/api".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "Scientific papers and research".to_string(),
                },
                DataSourceConfig {
                    name: "PubMed".to_string(),
                    base_url: "https://eutils.ncbi.nlm.nih.gov/entrez/eutils".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 3,
                    description: "Medical and life sciences research".to_string(),
                },
            ],
        );

        // ============= ENTERTAINMENT =============
        sources.insert(
            DataCategory::Entertainment,
            vec![
                DataSourceConfig {
                    name: "TMDB (Movies/TV)".to_string(),
                    base_url: "https://api.themoviedb.org/3".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 40,
                    description: "Movie and TV show database".to_string(),
                },
                DataSourceConfig {
                    name: "Spotify".to_string(),
                    base_url: "https://api.spotify.com/v1".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 180,
                    description: "Music metadata, playlists".to_string(),
                },
                DataSourceConfig {
                    name: "YouTube Data API".to_string(),
                    base_url: "https://www.googleapis.com/youtube/v3".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 100,
                    description: "YouTube videos, channels, statistics".to_string(),
                },
            ],
        );

        // ============= GAMING =============
        sources.insert(
            DataCategory::Gaming,
            vec![
                DataSourceConfig {
                    name: "Steam".to_string(),
                    base_url: "https://api.steampowered.com".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 200,
                    description: "Steam games, players, market".to_string(),
                },
                DataSourceConfig {
                    name: "RAWG".to_string(),
                    base_url: "https://api.rawg.io/api".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 60,
                    description: "Video game database".to_string(),
                },
            ],
        );

        // ============= SOCIAL MEDIA =============
        sources.insert(
            DataCategory::Social,
            vec![
                DataSourceConfig {
                    name: "Twitter/X API".to_string(),
                    base_url: "https://api.twitter.com/2".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 15,
                    description: "Tweets, trends (free tier limited)".to_string(),
                },
                DataSourceConfig {
                    name: "Mastodon".to_string(),
                    base_url: "https://mastodon.social/api/v1".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 300,
                    description: "Decentralized social media".to_string(),
                },
            ],
        );

        // ============= HEALTH =============
        sources.insert(
            DataCategory::Health,
            vec![
                DataSourceConfig {
                    name: "COVID-19 Data".to_string(),
                    base_url: "https://disease.sh/v3/covid-19".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 600,
                    description: "COVID-19 statistics worldwide".to_string(),
                },
                DataSourceConfig {
                    name: "OpenFDA".to_string(),
                    base_url: "https://api.fda.gov".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 240,
                    description: "FDA drug, device, and food data".to_string(),
                },
            ],
        );

        // ============= TRANSPORTATION =============
        sources.insert(
            DataCategory::Transportation,
            vec![DataSourceConfig {
                name: "FlightAware".to_string(),
                base_url: "https://aeroapi.flightaware.com/aeroapi".to_string(),
                requires_api_key: true,
                rate_limit_per_minute: 10,
                description: "Flight tracking (free tier limited)".to_string(),
            }],
        );

        // ============= FOOD/NUTRITION =============
        sources.insert(
            DataCategory::Food,
            vec![DataSourceConfig {
                name: "Open Food Facts".to_string(),
                base_url: "https://world.openfoodfacts.org/api/v0".to_string(),
                requires_api_key: false,
                rate_limit_per_minute: 100,
                description: "Food product database and nutrition".to_string(),
            }],
        );

        // ============= RANDOM DATA =============
        sources.insert(
            DataCategory::RandomData,
            vec![
                DataSourceConfig {
                    name: "Random.org".to_string(),
                    base_url: "https://www.random.org/integers".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1000,
                    description: "True random numbers (atmospheric noise)".to_string(),
                },
                DataSourceConfig {
                    name: "Lorem Picsum".to_string(),
                    base_url: "https://picsum.photos".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1000,
                    description: "Random placeholder images".to_string(),
                },
            ],
        );

        // ============= FUN (Jokes, Quotes, Facts) =============
        sources.insert(
            DataCategory::Fun,
            vec![
                DataSourceConfig {
                    name: "JokeAPI".to_string(),
                    base_url: "https://v2.jokeapi.dev".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 120,
                    description: "Free jokes API - programming, dark, puns, etc.".to_string(),
                },
                DataSourceConfig {
                    name: "Quotable".to_string(),
                    base_url: "https://api.quotable.io".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 180,
                    description: "Random quotes and inspirational sayings".to_string(),
                },
                DataSourceConfig {
                    name: "Chuck Norris Facts".to_string(),
                    base_url: "https://api.chucknorris.io".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Random Chuck Norris facts".to_string(),
                },
                DataSourceConfig {
                    name: "Useless Facts".to_string(),
                    base_url: "https://uselessfacts.jsph.pl".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Random useless but interesting facts".to_string(),
                },
            ],
        );

        // ============= GEOGRAPHY =============
        sources.insert(
            DataCategory::Geography,
            vec![
                DataSourceConfig {
                    name: "REST Countries".to_string(),
                    base_url: "https://restcountries.com/v3.1".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Country data: population, currency, languages".to_string(),
                },
                DataSourceConfig {
                    name: "Open Notify ISS".to_string(),
                    base_url: "http://api.open-notify.org".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "International Space Station current location".to_string(),
                },
                DataSourceConfig {
                    name: "Zippopotam.us".to_string(),
                    base_url: "http://api.zippopotam.us".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 60,
                    description: "Postal/ZIP code data for 60+ countries".to_string(),
                },
            ],
        );

        // ============= LANGUAGE =============
        sources.insert(
            DataCategory::Language,
            vec![
                DataSourceConfig {
                    name: "Free Dictionary".to_string(),
                    base_url: "https://api.dictionaryapi.dev/api/v2".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Word definitions, pronunciations, synonyms".to_string(),
                },
                DataSourceConfig {
                    name: "Fun Translations".to_string(),
                    base_url: "https://api.funtranslations.com".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 5,
                    description: "Translate to Yoda, Shakespeare, Pirate speak".to_string(),
                },
            ],
        );

        // ============= BOOKS =============
        sources.insert(
            DataCategory::Books,
            vec![DataSourceConfig {
                name: "Open Library".to_string(),
                base_url: "https://openlibrary.org/api".to_string(),
                requires_api_key: false,
                rate_limit_per_minute: 100,
                description: "Book data, ISBN lookup, author info".to_string(),
            }],
        );

        // ============= ANIMALS =============
        sources.insert(
            DataCategory::Animals,
            vec![
                DataSourceConfig {
                    name: "Cat Facts".to_string(),
                    base_url: "https://catfact.ninja".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Random cat facts".to_string(),
                },
                DataSourceConfig {
                    name: "Dog API".to_string(),
                    base_url: "https://dog.ceo/api".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Random dog images by breed".to_string(),
                },
                DataSourceConfig {
                    name: "HTTP Cats".to_string(),
                    base_url: "https://http.cat".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "HTTP status codes as cat images".to_string(),
                },
            ],
        );

        // ============= INTERNET =============
        sources.insert(
            DataCategory::Internet,
            vec![
                DataSourceConfig {
                    name: "IP-API".to_string(),
                    base_url: "http://ip-api.com/json".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 45,
                    description: "IP geolocation data".to_string(),
                },
                DataSourceConfig {
                    name: "IPify".to_string(),
                    base_url: "https://api.ipify.org".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1000,
                    description: "Get your public IP address".to_string(),
                },
            ],
        );

        // ============= ART =============
        sources.insert(
            DataCategory::Art,
            vec![
                DataSourceConfig {
                    name: "Rijksmuseum".to_string(),
                    base_url: "https://www.rijksmuseum.nl/api".to_string(),
                    requires_api_key: true,
                    rate_limit_per_minute: 60,
                    description: "Dutch masterpieces and art collections".to_string(),
                },
                DataSourceConfig {
                    name: "Color API".to_string(),
                    base_url: "https://www.thecolorapi.com".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Color schemes, palettes, conversions".to_string(),
                },
            ],
        );

        // ============= MATH =============
        sources.insert(
            DataCategory::Math,
            vec![DataSourceConfig {
                name: "Numbers API".to_string(),
                base_url: "http://numbersapi.com".to_string(),
                requires_api_key: false,
                rate_limit_per_minute: 100,
                description: "Interesting facts about numbers".to_string(),
            }],
        );

        // ============= EDUCATION =============
        sources.insert(
            DataCategory::Education,
            vec![DataSourceConfig {
                name: "Universities Hipolabs".to_string(),
                base_url: "http://universities.hipolabs.com".to_string(),
                requires_api_key: false,
                rate_limit_per_minute: 100,
                description: "University data worldwide".to_string(),
            }],
        );

        // ============= BLOCKCHAIN (Additional) =============
        sources.insert(
            DataCategory::Blockchain,
            vec![
                DataSourceConfig {
                    name: "Blockchain.com".to_string(),
                    base_url: "https://blockchain.info".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Bitcoin blockchain data, transactions".to_string(),
                },
                DataSourceConfig {
                    name: "CoinDesk BPI".to_string(),
                    base_url: "https://api.coindesk.com/v1/bpi".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 100,
                    description: "Bitcoin Price Index".to_string(),
                },
                DataSourceConfig {
                    name: "ExchangeRate-API".to_string(),
                    base_url: "https://open.er-api.com/v6".to_string(),
                    requires_api_key: false,
                    rate_limit_per_minute: 1500,
                    description: "Currency exchange rates".to_string(),
                },
            ],
        );

        Self { sources }
    }

    /// Get all sources for a category
    pub fn get_sources(&self, category: &DataCategory) -> Vec<DataSourceConfig> {
        self.sources.get(category).cloned().unwrap_or_default()
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<DataCategory> {
        self.sources.keys().cloned().collect()
    }

    /// Get total number of data sources
    pub fn total_sources(&self) -> usize {
        self.sources.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_source_catalog() {
        let catalog = DataSourceCatalog::new();

        // Should have sources for all categories
        assert!(catalog.total_sources() > 30);

        // Crypto should have multiple sources
        let crypto_sources = catalog.get_sources(&DataCategory::Cryptocurrency);
        assert!(crypto_sources.len() >= 5);

        // Weather sources
        let weather_sources = catalog.get_sources(&DataCategory::Weather);
        assert!(weather_sources.len() >= 2);
    }
}
