#![allow(dead_code)]
//! This is to generate global identifier

use std::hint::spin_loop;
use std::ops::Deref;
use std::sync::atomic::{AtomicI64, Ordering};

use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::Visitor;
use serde::{de, Serialize, Serializer};

#[derive(Debug)]
pub struct NumericalUniqueIdGenerator {
	/// epoch used by the snowflake algorithm.
	epoch: SystemTime,

	/// datacenter_id and machine_id are fixed once the system is up running.
	/// Any changes in datacenter IDs require careful review since an accidental change in those values can lead to ID conflicts
	/// make sure that none of them is bigger than 5bits
	pub datacenter_id: i32,
	pub machine_id: i32,

	/// timpstamp(42bits) + sequence number(12bits).
	ts_seq: AtomicI64,
}

struct TimeStampSeq {
	ts: i64,
	seq: i64,
}
impl TimeStampSeq {
	fn to_i64(&self) -> i64 {
		// combine the timestamp and sequence number.
		// It's basically the reverse of the `current_ts_seq` function.
		self.ts << 12 | self.seq
	}

	fn next_ts_seq(&self, current_ts: i64, epoch: SystemTime) -> Self {
		const MAX_SEQUENCE: i64 = 4095;
		let (ts, seq) = if self.ts == current_ts {
			// If timestamp hasn't changed, increment sequence
			let next_seq = self.seq.wrapping_add(1) & MAX_SEQUENCE;

			// If sequence overflows, move to next millisecond
			if next_seq == 0 {
				let next_ts = race_next_milli(current_ts, epoch);
				(next_ts, 0)
			} else {
				(current_ts, next_seq)
			}
		} else {
			(current_ts, 0)
		};

		TimeStampSeq { ts, seq }
	}
}

#[derive(Debug)]
pub struct NumericalUniqueIdBucket {
	/// Hidden the `NumericalUniqueIdGenerator` in bucket .
	snowflake_id_generator: NumericalUniqueIdGenerator,

	/// The bucket buffer;
	bucket: Vec<i64>,
}

impl NumericalUniqueIdGenerator {
	/// Constructs a new `NumericalUniqueIdGenerator` using the UNIX epoch.
	///
	/// # Examples
	///
	/// ```
	/// use snowflake::NumericalUniqueIdGenerator;
	///
	/// let id_generator = NumericalUniqueIdGenerator::new(1, 1);
	/// ```
	pub fn new(datacenter_id: i32, machine_id: i32) -> NumericalUniqueIdGenerator {
		Self::with_epoch(datacenter_id, machine_id, UNIX_EPOCH)
	}

	fn ts_seq(&self) -> TimeStampSeq {
		const MASKING: i64 = 0xFFF;
		let ts_seq = self.ts_seq.load(Ordering::Relaxed);
		TimeStampSeq {
			ts: ts_seq >> 12,
			seq: ts_seq & MASKING,
		}
	}

	/// Constructs a new `NumericalUniqueIdGenerator` using the specified epoch.
	///
	/// # Examples
	///
	/// ```
	/// use std::time::{Duration, UNIX_EPOCH};
	/// use snowflake::NumericalUniqueIdGenerator;
	///
	/// // 1 January 2015 00:00:00
	/// let discord_epoch = UNIX_EPOCH + Duration::from_millis(1420070400000);
	/// let id_generator = NumericalUniqueIdGenerator::with_epoch(1, 1, discord_epoch);
	/// ```
	pub fn with_epoch(datacenter_id: i32, machine_id: i32, epoch: SystemTime) -> NumericalUniqueIdGenerator {
		//TODO:limit the maximum of input args datacenter_id and machine_id
		let timestamp = current_time_in_milli(epoch);

		NumericalUniqueIdGenerator {
			epoch,
			datacenter_id,
			machine_id,
			ts_seq: AtomicI64::new(timestamp << 12),
		}
	}

	/// within 64 bits:
	/// sign bit and timestamp takes 42 bits so, left shift 22
	/// datacenter id takes 5 bits in the second place so left shift 17
	/// machine id takes 5 bits in the third place so left shift 12
	/// sequence number comes last.
	fn get_snowflake(&self, ts_seq: TimeStampSeq) -> i64 {
		ts_seq.ts << 22 | ((self.datacenter_id as i64) << 17) | ((self.machine_id as i64) << 12) | (ts_seq.seq)
	}

	/// The basic guarantee time punctuality.
	///
	/// Basic guarantee time punctuality.

	/// When traffic peaks, 4096 in a millsec is simply not enough.
	/// But setting time after every 4096 calls.
	///
	/// # Examples
	///
	/// ```
	/// use snowflake::NumericalUniqueIdGenerator;
	///
	/// let mut id_generator = NumericalUniqueIdGenerator::new(1, 1);
	/// id_generator.generate();
	/// ```
	pub fn generate(&self) -> i64 {
		loop {
			let last_ts_seq = self.ts_seq();
			let current_ts = current_time_in_milli(self.epoch);

			//This condition handles clock drift or system time adjustments where the clock moves backwards.
			//It occurs in scenarios like: NTP adjustments, Daylight saving, Manual system clock adjustment. VM time sync
			if last_ts_seq.ts > current_ts {
				spin_loop();
				continue;
			}

			let new_ts_seq = last_ts_seq.next_ts_seq(current_ts, self.epoch);

			if let Ok(_) = self.ts_seq.compare_exchange(last_ts_seq.to_i64(), new_ts_seq.to_i64(), Ordering::Relaxed, Ordering::Relaxed) {
				return self.get_snowflake(new_ts_seq);
			}
		}
	}
}

// TODO Get the following concept
impl NumericalUniqueIdBucket {
	/// Constructs a new `NumericalUniqueIdBucket` using the UNIX epoch.
	/// Please make sure that datacenter_id and machine_id is small than 32(2^5);
	///
	/// # Examples
	///
	/// ```
	/// use snowflake::NumericalUniqueIdBucket;
	///
	/// let id_generator_bucket = NumericalUniqueIdBucket::new(1, 1);
	/// ```
	pub fn new(datacenter_id: i32, machine_id: i32) -> Self {
		Self::with_epoch(datacenter_id, machine_id, UNIX_EPOCH)
	}

	/// Constructs a new `NumericalUniqueIdBucket` using the specified epoch.
	/// Please make sure that datacenter_id and machine_id is small than 32(2^5);
	///
	/// # Examples
	///
	/// ```
	/// use std::time::{Duration, UNIX_EPOCH};
	/// use snowflake::NumericalUniqueIdBucket;
	///
	/// // 1 January 2015 00:00:00
	/// let beringlab = UNIX_EPOCH + Duration::from_millis(1570292856000);
	/// let id_generator_bucket = NumericalUniqueIdBucket::with_epoch(1, 1, beringlab);
	/// ```
	pub fn with_epoch(datacenter_id: i32, machine_id: i32, epoch: SystemTime) -> Self {
		let snowflake_id_generator = NumericalUniqueIdGenerator::with_epoch(datacenter_id, machine_id, epoch);
		let bucket = Vec::new();

		NumericalUniqueIdBucket { snowflake_id_generator, bucket }
	}

	/// # Examples
	///
	/// ```
	/// use snowflake::NumericalUniqueIdBucket;
	///
	/// let mut id_generator_bucket = NumericalUniqueIdBucket::new(1, 1);
	/// let id = id_generator_bucket.get_id();
	///
	/// ```
	pub fn get_id(&mut self) -> i64 {
		// 247 ns/iter
		// after self.bucket.push(self.snowflake_id_generator.generate());

		if self.bucket.is_empty() {
			self.fill_bucket();
		}
		self.bucket.pop().unwrap()
	}

	fn fill_bucket(&mut self) {
		// 1,107,103 -- 1,035,018 ns/iter
		//self.bucket.push(self.snowflake_id_generator.generate());

		for _ in 0..4091 {
			self.bucket.push(self.snowflake_id_generator.generate());
		}
	}
}

#[inline(always)]
/// Get the latest milliseconds of the clock.
pub fn current_time_in_milli(epoch: SystemTime) -> i64 {
	SystemTime::now().duration_since(epoch).expect("System Time Error!").as_millis() as i64
}

#[inline(always)]
// Constantly refreshing the latest milliseconds by busy waiting.
fn race_next_milli(timestamp: i64, epoch: SystemTime) -> i64 {
	let mut latest_time_millis: i64;
	loop {
		latest_time_millis = current_time_in_milli(epoch);
		if latest_time_millis > timestamp {
			return latest_time_millis;
		}
		spin_loop();
	}
}

static ID_GENERATOR: std::sync::LazyLock<NumericalUniqueIdGenerator> = std::sync::LazyLock::new(|| {
	NumericalUniqueIdGenerator::new(
		std::env::var("DATACENTER_ID").unwrap_or("1".to_string()).parse::<i32>().expect("Parsing Failed!"),
		std::env::var("MACHINE_ID").unwrap_or("1".to_string()).parse::<i32>().expect("Parsing Failed!"),
	)
});

#[derive(Clone, Hash, PartialEq, Debug, Eq, Ord, PartialOrd, Copy, Default)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SnowFlake(pub i64);
impl SnowFlake {
	pub fn generate() -> Self {
		ID_GENERATOR.generate().into()
	}
}

impl Deref for SnowFlake {
	type Target = i64;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl From<i64> for SnowFlake {
	fn from(value: i64) -> Self {
		Self(value)
	}
}

impl From<SnowFlake> for String {
	fn from(value: SnowFlake) -> Self {
		value.0.to_string()
	}
}

impl From<SnowFlake> for i64 {
	fn from(value: SnowFlake) -> Self {
		value.0
	}
}

impl std::fmt::Display for SnowFlake {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl<'de> serde::Deserialize<'de> for SnowFlake {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct SnowflakeVisitor;

		impl<'de> Visitor<'de> for SnowflakeVisitor {
			type Value = SnowFlake;

			fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
				f.write_str("Snowflake as a number or string")
			}

			fn visit_i64<E>(self, id: i64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(SnowFlake(id))
			}

			fn visit_u64<E>(self, id: u64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				if id < i64::MAX as u64 {
					Ok(SnowFlake(id.try_into().unwrap()))
				} else {
					Err(E::custom(format!("Snowflake out of range: {}", id)))
				}
			}

			fn visit_str<E>(self, id: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				match id.parse::<u64>() {
					Ok(val) => self.visit_u64(val),
					Err(_) => Err(E::custom("Failed to parse snowflake")),
				}
			}
		}

		deserializer.deserialize_any(SnowflakeVisitor)
	}
}

impl Serialize for SnowFlake {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// Convert the u64 to a string
		let s = self.0.to_string();

		// Serialize the string
		serializer.serialize_str(&s)
	}
}

#[test]
fn test_generate() {
	let id_generator = NumericalUniqueIdGenerator::new(1, 2);
	let mut ids = Vec::with_capacity(10000);
	let id_count = 100000;

	for _ in 0..99 {
		for _ in 0..id_count {
			ids.push(id_generator.generate());
		}

		ids.sort();
		ids.dedup();

		assert_eq!(id_count, ids.len());

		ids.clear();
	}
}

#[test]
fn test_concurrent_id_generation() {
	use std::collections::HashSet;
	use std::thread;
	let thread_count = 20;
	let id_count = 10000;

	let mut handles = vec![];
	for thread_id in 0..thread_count {
		let handle = thread::spawn(move || {
			let mut ids = HashSet::with_capacity(id_count);
			for _ in 0..id_count {
				let id = SnowFlake::generate();
				let res = ids.insert(id);
				assert!(res, "thread_id: {}, id already exists: {:b}", thread_id, id.0);
			}
			ids.len()
		});
		handles.push(handle);
	}
	for handle in handles {
		handle.join().unwrap();
	}
}

#[test]
fn test_generate_not_sequential_value_when_sleep() {
	let id_generator = NumericalUniqueIdGenerator::new(1, 2);
	let first = id_generator.generate();

	std::thread::sleep(std::time::Duration::from_millis(1));
	let second = id_generator.generate();

	assert!(first < second);
	assert_ne!(first + 1, second);
}

#[test]
fn test_singleton_generate() {
	let id_generator = &ID_GENERATOR;
	let mut ids = Vec::with_capacity(1000000);

	for _ in 0..99 {
		for _ in 0..1000000 {
			ids.push(id_generator.generate());
		}

		assert_eq!(1000000, ids.len());
		assert!(ids.first().unwrap() < ids.last().unwrap());
		assert!(ids.get(999998).unwrap() < ids.get(999999).unwrap());

		ids.clear();
	}
}
