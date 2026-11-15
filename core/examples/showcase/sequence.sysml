// Sequence view (sv): four lifelines and fourteen sends ordered by their source lines.
// This demonstrates explicit sends only, not paired accepts, event markers or nested sequence views.
package MaritimeExchange {
    item def MissionPacket;
    item missionPlan : MissionPacket;
    item departureNotice : MissionPacket;
    item acquisitionCommand : MissionPacket;
    item soundingBatch : MissionPacket;
    item healthReport : MissionPacket;
    item relayRequest : MissionPacket;
    item receipt : MissionPacket;
    item returnCommand : MissionPacket;

    part def ShoreStation;
    part def SurveyVessel;
    part def SonarArray;
    part def RelayBuoy;

    part shore : ShoreStation {
        action dispatch {
            send missionPlan to craft;
            send relayRequest to buoy;
        }

        part craft : SurveyVessel {
            action depart {
                send departureNotice to shore;
                send acquisitionCommand to sonar;
            }

            part sonar : SonarArray {
                action acquire {
                    send healthReport to craft;
                    send soundingBatch to craft;
                    send soundingBatch to craft;
                }
            }

            action upload {
                send soundingBatch to buoy;
                send healthReport to shore;
                send receipt to sonar;
            }
        }

        part buoy : RelayBuoy {
            action relay {
                send soundingBatch to shore;
                send receipt to craft;
            }
        }

        action recover {
            send returnCommand to craft;
            send receipt to buoy;
        }
    }
}
