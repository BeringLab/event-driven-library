use ruva::*;
use ruva_macro::into_command;

#[test]
fn test_into_command_body_command_specified() {
	#[into_command(body(Serialize), command(Serialize, serde::Deserialize))]
	struct SomeCommand {
		#[required_input]
		id: i32,
		name: String,
		foo: i32,
	}

	let command = SomeCommandBody { name: "migo".into(), foo: 2 };
	let serilaized = serde_json::to_string(&command).unwrap();
	let deserialized: SomeCommandBody = serde_json::from_str::<SomeCommandBody>(&serilaized).unwrap();
	assert_eq!(format!("{:?}", deserialized), "SomeCommandBody { name: \"migo\", foo: 2 }".to_string());

	let command2 = deserialized.into_command(1);
	assert_eq!(format!("{:?}", command2), "SomeCommand { id: 1, name: \"migo\", foo: 2 }".to_string());
	let _ = serde_json::from_str::<SomeCommand>(&serde_json::to_string(&command2).unwrap()).unwrap();
}

#[test]
fn test_into_command_only_body_specified() {
	#[into_command(body(Serialize))]
	struct SomeCommand {
		#[required_input]
		id: i32,
		name: String,
		foo: i32,
	}

	let command = SomeCommandBody { name: "migo".into(), foo: 2 };
	let serilaized = serde_json::to_string(&command).unwrap();

	let deserialized: SomeCommandBody = serde_json::from_str::<SomeCommandBody>(&serilaized).unwrap();
	assert_eq!(format!("{:?}", deserialized), "SomeCommandBody { name: \"migo\", foo: 2 }".to_string());

	let command2: SomeCommand = deserialized.into_command(1);
	assert_eq!(format!("{:?}", command2), "SomeCommand { id: 1, name: \"migo\", foo: 2 }".to_string());
}

#[test]
fn test_into_command_nothing_specified() {
	#[into_command]
	struct SomeCommand {
		#[required_input]
		id: i32,
		name: String,
		foo: i32,
	}

	let deserialized: SomeCommandBody = serde_json::from_str("{\"name\":\"migo\",\"foo\":2}").unwrap();
	let commnand = deserialized.into_command(1);
	let _serialized = serde_json::to_string(&commnand).unwrap();
}

#[test]
fn test_into_command_with_generic() {
	// Even without the `Sync`, `Send` and `Debug` contraint on generic type T, it's still valid.

	#[into_command(body(Serialize))]
	struct SomeCommand<T: Serialize> {
		#[required_input]
		id: i32,
		name: String,
		foo: i32,
		t_field: T,
	}

	let command = SomeCommandBody::<i32> { name: "migo".into(), foo: 2, t_field: 1 };
	let serilaized = serde_json::to_string(&command).unwrap();
	let deserialized: SomeCommandBody<i32> = serde_json::from_str::<SomeCommandBody<i32>>(&serilaized).unwrap();
	assert_eq!(format!("{:?}", deserialized), "SomeCommandBody { name: \"migo\", foo: 2, t_field: 1 }".to_string());

	let command2 = deserialized.into_command(1);
	assert_eq!(format!("{:?}", command2), "SomeCommand { id: 1, name: \"migo\", foo: 2, t_field: 1 }".to_string());
}

#[test]
fn test_unit_command() {
	#[into_command]
	struct UnitCommand;
	fn fn_accept_t_command<T: ruva::TCommand>(_: T) {}
	fn_accept_t_command(UnitCommand);
}

#[test]
fn test_into_command_with_tevent() {
	#[aggregate]
	struct SomeOtherThing {
		id: i32,
	}

	#[allow(dead_code)]
	#[warn(deprecated)]
	#[deny(invalid_atomic_ordering)]
	#[into_command(command(ruva::TEvent, Clone,))]
	#[externally_notifiable(SomeOtherThing)]
	#[internally_notifiable]
	struct SomeCommand {
		#[required_input]
		#[identifier]
		id: i32,
		#[serde(rename = "Name")]
		name: String,
		foo: i32,
	}

	let command = SomeCommand { id: 1, name: "migo".into(), foo: 2 };

	let metadata = command.metadata();
	assert_eq!(metadata.aggregate_id, "1");
	assert_eq!(metadata.aggregate_name, "SomeOtherThing");
	assert_eq!(metadata.topic, "SomeCommand");
	assert!(command.externally_notifiable());
	assert!(command.internally_notifiable());

	let serilaized = serde_json::to_string(&command).unwrap();
	assert_eq!(serilaized, "{\"id\":1,\"Name\":\"migo\",\"foo\":2}".to_string());
}
