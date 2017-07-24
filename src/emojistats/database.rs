extern crate postgres;

pub struct Database {
    conn: postgres::Connection,
}

impl Database {
    pub fn new<T>(params: T) -> postgres::Result<Database>
        where T: postgres::params::IntoConnectParams
    {
        let conn = postgres::Connection::connect(params, postgres::TlsMode::None)?;

        Ok(Database { conn })
    }
}
