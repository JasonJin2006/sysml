package FuelSystem {
    private import ScalarValues::*;

    port def FuelOutlet {
        out fuel : Real;
    }

    port def FuelInlet {
        in fuel : Real;
    }

    part def Tank {
        port outlet : FuelOutlet;
        attribute capacity : Real;
    }

    part def Engine {
        port inlet : FuelInlet;
        attribute thrust : Real;
    }

    part def Vehicle {
        part tank : Tank;
        part engine : Engine;
        connect tank.outlet to engine.inlet;
    }

    part vehicle : Vehicle;
}
