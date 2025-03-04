/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::rc::Rc;

use bluetooth_traits::{BluetoothResponse, GATTType};
use dom_struct::dom_struct;

use crate::dom::bindings::codegen::Bindings::BluetoothRemoteGATTServerBinding::BluetoothRemoteGATTServerMethods;
use crate::dom::bindings::codegen::Bindings::BluetoothRemoteGATTServiceBinding::BluetoothRemoteGATTServiceMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::reflector::reflect_dom_object;
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::DOMString;
use crate::dom::bluetooth::{AsyncBluetoothListener, get_gatt_children};
use crate::dom::bluetoothdevice::BluetoothDevice;
use crate::dom::bluetoothuuid::{BluetoothCharacteristicUUID, BluetoothServiceUUID, BluetoothUUID};
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::promise::Promise;
use crate::script_runtime::CanGc;

// https://webbluetoothcg.github.io/web-bluetooth/#bluetoothremotegattservice
#[dom_struct]
pub(crate) struct BluetoothRemoteGATTService {
    eventtarget: EventTarget,
    device: Dom<BluetoothDevice>,
    uuid: DOMString,
    is_primary: bool,
    instance_id: String,
}

impl BluetoothRemoteGATTService {
    pub(crate) fn new_inherited(
        device: &BluetoothDevice,
        uuid: DOMString,
        is_primary: bool,
        instance_id: String,
    ) -> BluetoothRemoteGATTService {
        BluetoothRemoteGATTService {
            eventtarget: EventTarget::new_inherited(),
            device: Dom::from_ref(device),
            uuid,
            is_primary,
            instance_id,
        }
    }

    #[allow(non_snake_case)]
    pub(crate) fn new(
        global: &GlobalScope,
        device: &BluetoothDevice,
        uuid: DOMString,
        isPrimary: bool,
        instanceID: String,
        can_gc: CanGc,
    ) -> DomRoot<BluetoothRemoteGATTService> {
        reflect_dom_object(
            Box::new(BluetoothRemoteGATTService::new_inherited(
                device, uuid, isPrimary, instanceID,
            )),
            global,
            can_gc,
        )
    }

    fn get_instance_id(&self) -> String {
        self.instance_id.clone()
    }
}

impl BluetoothRemoteGATTServiceMethods<crate::DomTypeHolder> for BluetoothRemoteGATTService {
    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-device
    fn Device(&self) -> DomRoot<BluetoothDevice> {
        DomRoot::from_ref(&self.device)
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-isprimary
    fn IsPrimary(&self) -> bool {
        self.is_primary
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-uuid
    fn Uuid(&self) -> DOMString {
        self.uuid.clone()
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-getcharacteristic
    fn GetCharacteristic(
        &self,
        characteristic: BluetoothCharacteristicUUID,
        can_gc: CanGc,
    ) -> Rc<Promise> {
        get_gatt_children(
            self,
            true,
            BluetoothUUID::characteristic,
            Some(characteristic),
            self.get_instance_id(),
            self.Device().get_gatt().Connected(),
            GATTType::Characteristic,
            can_gc,
        )
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-getcharacteristics
    fn GetCharacteristics(
        &self,
        characteristic: Option<BluetoothCharacteristicUUID>,
        can_gc: CanGc,
    ) -> Rc<Promise> {
        get_gatt_children(
            self,
            false,
            BluetoothUUID::characteristic,
            characteristic,
            self.get_instance_id(),
            self.Device().get_gatt().Connected(),
            GATTType::Characteristic,
            can_gc,
        )
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-getincludedservice
    fn GetIncludedService(&self, service: BluetoothServiceUUID, can_gc: CanGc) -> Rc<Promise> {
        get_gatt_children(
            self,
            false,
            BluetoothUUID::service,
            Some(service),
            self.get_instance_id(),
            self.Device().get_gatt().Connected(),
            GATTType::IncludedService,
            can_gc,
        )
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-bluetoothremotegattservice-getincludedservices
    fn GetIncludedServices(
        &self,
        service: Option<BluetoothServiceUUID>,
        can_gc: CanGc,
    ) -> Rc<Promise> {
        get_gatt_children(
            self,
            false,
            BluetoothUUID::service,
            service,
            self.get_instance_id(),
            self.Device().get_gatt().Connected(),
            GATTType::IncludedService,
            can_gc,
        )
    }

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-serviceeventhandlers-onserviceadded
    event_handler!(serviceadded, GetOnserviceadded, SetOnserviceadded);

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-serviceeventhandlers-onservicechanged
    event_handler!(servicechanged, GetOnservicechanged, SetOnservicechanged);

    // https://webbluetoothcg.github.io/web-bluetooth/#dom-serviceeventhandlers-onserviceremoved
    event_handler!(serviceremoved, GetOnserviceremoved, SetOnserviceremoved);
}

impl AsyncBluetoothListener for BluetoothRemoteGATTService {
    fn handle_response(&self, response: BluetoothResponse, promise: &Rc<Promise>, can_gc: CanGc) {
        let device = self.Device();
        match response {
            // https://webbluetoothcg.github.io/web-bluetooth/#getgattchildren
            // Step 7.
            BluetoothResponse::GetCharacteristics(characteristics_vec, single) => {
                if single {
                    promise.resolve_native(
                        &device.get_or_create_characteristic(&characteristics_vec[0], self, can_gc),
                        can_gc,
                    );
                    return;
                }
                let mut characteristics = vec![];
                for characteristic in characteristics_vec {
                    let bt_characteristic =
                        device.get_or_create_characteristic(&characteristic, self, can_gc);
                    characteristics.push(bt_characteristic);
                }
                promise.resolve_native(&characteristics, can_gc);
            },
            // https://webbluetoothcg.github.io/web-bluetooth/#getgattchildren
            // Step 7.
            BluetoothResponse::GetIncludedServices(services_vec, single) => {
                if single {
                    return promise.resolve_native(
                        &device.get_or_create_service(&services_vec[0], &device.get_gatt(), can_gc),
                        can_gc,
                    );
                }
                let mut services = vec![];
                for service in services_vec {
                    let bt_service =
                        device.get_or_create_service(&service, &device.get_gatt(), can_gc);
                    services.push(bt_service);
                }
                promise.resolve_native(&services, can_gc);
            },
            _ => promise.reject_error(Error::Type("Something went wrong...".to_owned()), can_gc),
        }
    }
}
