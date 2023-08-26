/*
 * TODO later
 * [] Add tests
 */
use calamine::{open_workbook, Error, Xlsx, Reader};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Trade {
    executions: Vec<Execution>,
    pnl: f64,
    commission: f64,
    entry_time: String,
    exit_time: String,
    instrument: String,
    long: bool,
}

#[derive(Debug, Clone)]
struct Execution {
    time: String,
    price: f64,
    action: String,
    quantity: u32,
    commission: f64,
    instrument: String,
}

fn main() -> Result<(), Error> {
    let _args: Vec<String> = std::env::args().collect();
    //let path = args.get(1).expect("expected a file path");
    //let user_id = args.get(2).expect("expected a user_id");
    let paths = std::fs::read_dir("trades").unwrap().map(|path| path.unwrap().path());
    let mut account_executions: HashMap<String, HashMap<String,Vec<Execution>>> = HashMap::new();
    for path in paths {
        if path.to_str().unwrap() == "trades/.DS_Store" {
            continue;
        }
        let mut workbook: Xlsx<_> = open_workbook(path.to_str().unwrap())?;
        let range = workbook.worksheet_range("Sheet1")
            .ok_or(Error::Msg("Cannot find 'Sheet1'"))??;
        let mut rows = range.rows();
        rows.next();
        for r in rows {
            let instrument = r[0].to_string();
            let instrument_str = instrument.split_whitespace().next().unwrap_or("");
            let time = r[4].to_string();
            let _datetime = &r[4].as_datetime().unwrap();
            let action = r[1].get_string().unwrap();
            let quantity: u32 = r[2].to_string().parse().unwrap();
            let price: f64 = r[3].to_string().parse().unwrap();
            let commission = r[10].to_string()[1..].parse().unwrap();
            let account_display_name = r[12].to_string();
            let account = account_executions.entry(account_display_name).or_default();
            account.entry(instrument_str.to_string())
                .and_modify(|e| {
                    e.push(Execution {
                        time: time.to_string(),
                        price,
                        action: action.to_string(),
                        quantity,
                        commission,
                        instrument: instrument_str.to_string(),
                    });
                })
                .or_insert_with(|| {
                    Vec::from([Execution {
                        time,
                        price,
                        action: action.to_string(),
                        quantity,
                        commission,
                        instrument: instrument_str.to_string(),
                    }])
                });
        }
    }
    
    for account in account_executions.iter_mut() {
        for instrument in account.1.iter_mut() {
            instrument.1.sort_by_key(|e| e.time.to_string() );
            let (mut slow, mut fast) = (0, 1);
            let n = instrument.1.len();
            while fast < n {
                let base = &instrument.1[slow];
                /* TODO .clone() is definitely wrong here, will fix this later */
                let runner = &instrument.1[fast].clone();
                if base.time == runner.time && base.action == runner.action && base.instrument == runner.instrument {
                    instrument.1[slow].quantity += runner.quantity;
                    instrument.1[slow].commission += runner.commission;
                } else {
                    slow += 1;
                    instrument.1[slow] = runner.clone();
                }
                fast += 1;
            }
            instrument.1.truncate(slow + 1);
        }
    }
    let mut trades: HashMap<String, Vec<Trade>> = HashMap::new();
    for (account, instruments) in account_executions.iter_mut() {
        trades.insert(account.to_string(), Vec::new());
        for (instrument, executions) in instruments.iter_mut() {
            let mut positions: Vec<&mut Execution> = Vec::new();
            executions.sort_by_key(|e| e.time.to_string() );
            for e in executions.iter_mut() {
                let mut q = e.quantity;
                let commission_price = e.commission / q as f64;
                if !positions.is_empty() && positions.last().unwrap().action != e.action {
                    match q.cmp(&positions.last().unwrap().quantity) {
                        std::cmp::Ordering::Equal => {
                            let lastt = trades.get_mut(account).unwrap().last_mut().expect("Unable to get last trade in equal");
                            let lastp = positions.last_mut().expect("Unable to get last position in equal");
                            lastt.commission += e.commission;
                            lastt.pnl += calculate_pnl(lastp.price, e.price, q as f64, instrument, lastt.long);
                            lastt.exit_time = e.time.to_string();
                            lastt.executions.push(e.clone());
                            positions.pop();
                            q = 0;
                        }
                        std::cmp::Ordering::Less => {
                            let lastt = trades.get_mut(account).unwrap().last_mut().expect("Unable to get last trade in less");
                            let lastp = positions.last_mut().expect("Unable to get last position in less");
                            lastt.commission += e.commission;
                            lastt.pnl += calculate_pnl(lastp.price, e.price, q as f64, instrument, lastt.long);
                            lastt.exit_time = e.time.to_string();
                            lastt.executions.push(e.clone());
                            lastp.quantity -= q;
                            q = 0;
                        }
                        _ => {
                            let lastt = trades.get_mut(account).unwrap().last_mut().expect("Unable to get last trade in greater");
                            while !positions.is_empty() && q >= positions.last().unwrap().quantity {
                                let lastp = positions.last_mut().expect("Unable to get last position in greater");
                                lastt.pnl += calculate_pnl(lastp.price, e.price, lastp.quantity as f64, instrument, lastt.long);
                                q -= lastp.quantity;
                                positions.pop();
                            }
                            if !positions.is_empty() && q > 0 {
                                let lastp = positions.last_mut().expect("unable to get last position in greater if block");
                                lastt.pnl += calculate_pnl(lastp.price, e.price, lastp.quantity as f64, instrument, lastt.long);
                                lastp.quantity -= q;
                                q = 0;
                            }
                            let mut ec = e.clone();
                            let lastt = trades.get_mut(account).unwrap().last_mut().expect("Unable to get last trade in equal");
                            ec.quantity -= q;
                            ec.commission = ec.quantity as f64 * commission_price;
                            lastt.commission += ec.commission;
                            lastt.exit_time = ec.time.to_string();
                            lastt.executions.push(ec);
                        }
                    }
                    e.quantity = q;
                    e.commission = e.quantity as f64 * commission_price;
                }
                if q > 0 {
                    if positions.is_empty() {
                        trades.get_mut(account).unwrap().push(Trade {
                            pnl: 0.0,
                            commission: 0.0,
                            long: e.action == "Buy",
                            entry_time: e.time.to_string(),
                            exit_time: e.time.to_string(),
                            instrument: instrument.to_string(),
                            executions: Vec::from([]),
                        });
                    }
                    let lastt = trades.get_mut(account).unwrap().last_mut().expect("Unable to get last trade in adding");
                    lastt.commission += commission_price * e.quantity as f64;
                    lastt.executions.push(e.clone());
                    positions.push(e);
                }
            }
        }
    }
    println!(
"with role_id as (
    select id from roles
    where name = 'super_admin'
)
insert into users (id, username, password_hash, email, role_id)
values ('6982c6df-3d03-4583-8fa9-07386cf25f80', 'jhg', '$argon2id$v=19$m=15000,t=2,p=1$xjnT0gsfJCccXoCt8yD1HQ$rDkvWPpNR+yYNQ+U7+0U6RCLcgG/EnPPE3riQ615AFM', 'g@jinz.co', (select id from role_id))
on conflict (\"id\") do nothing;\n"
        );
    for (account, account_trades) in trades {
        println!(
"insert into accounts (user_id, name, sim)
values ('{0}', '{1}', {2})
on conflict (\"name\") do nothing;\n", "6982c6df-3d03-4583-8fa9-07386cf25f80", account, account == "Sim101"
);
        for trade in account_trades {
            let executions = trade.executions;
            let executions_string = executions
                .iter()
                .map(|e| {
                    format!("((select id from t_id), (select id from i_id), {}, {}, {}, '{}', {})", e.time, e.commission, e.price, e.action.to_lowercase() == "buy", e.quantity)
                })
                .collect::<Vec<String>>()
                .join(",\n");
            println!(
"insert into instruments (code, price_per_point)
values ('{0}', {1})
on conflict (\"code\") do nothing;\n", trade.instrument, point_prices()[&trade.instrument.as_str()]);

            println!(
"with acc_id as (
    select id from accounts
    where name = '{0}'
), i_id as (
    select id from instruments
    where code = '{1}'
), t_id as (
    insert into trades (account_id, instrument_id, entry_time, exit_time, commissions, pnl, is_short)
    values ((select id from acc_id), (select id from i_id), {2}, {3}, {4}, {5}, {6})
    returning id
)
insert into executions (trade_id, instrument_id, fill_time, commissions, price, is_buy, quantity)
values {7};\n\n", account, trade.instrument, trade.entry_time, trade.exit_time, trade.commission, trade.pnl, !trade.long, executions_string
);
        }
    }
    Ok(())
}

fn point_prices() -> HashMap<&'static str, f64> {
    HashMap::from([
                  ("ES", 50.0),
                  ("MES", 5.0),
                  ("NQ", 20.0),
                  ("MNQ", 2.0),
    ])
}

fn tick_prices() -> HashMap<&'static str, f64> {
    HashMap::from([
                  ("ES", 12.5),
                  ("MES", 1.25),
                  ("NQ", 5.0),
                  ("MNQ", 0.5),
    ])
}

fn calculate_pnl(entry: f64, exit: f64, quantity: f64, instrument: &str, long: bool) -> f64 {
    
    let ticks = quantity * 4.0 * (if long {1.0} else {-1.0});
    return (exit - entry) * ticks * tick_prices()[instrument];
}

