// Minimal stand-in for the SysML v2 standard library.
//
// Names and package layout follow the real library (the kernel `ScalarValues` package and the
// `ISQ`/`ISQBase`/`ISQMechanics`/`ISQThermodynamics` split of the Quantities and Units domain
// library). Only what the name resolver needs is here: one `attribute def` per scalar value
// type, so that `import ISQ::*` binds `MassValue` and friends to real elements.
//
// Deliberately not modelled yet, and why:
//   * specialization (`:> ScalarQuantityValue`) -- the element tree has no specialization
//     relation; it arrives with `sysml-units`, which is what actually needs the chain;
//   * units and quantities (`SI::kg`, `MeasurementReferences`) -- same reason;
//   * everything outside the names the examples import.
//
// Because the file is parsed by the same front end as user models, a syntax error here fails
// `load_project`, which is the point: the library cannot silently rot.

standard library package ScalarValues {
    attribute def ScalarValue;
    attribute def Boolean;
    attribute def String;
    attribute def NumericalValue;
    attribute def Number;
    attribute def Complex;
    attribute def Real;
    attribute def Rational;
    attribute def Integer;
    attribute def Natural;
    attribute def Positive;
}

standard library package ISQBase {
    attribute def LengthValue;
    attribute def DurationValue;
    attribute def TimeValue;
    attribute def MassValue;
    attribute def SpeedValue;
    attribute def VelocityValue;
    attribute def AccelerationValue;
    attribute def AreaValue;
    attribute def VolumeValue;
    attribute def TemperatureValue;
    attribute def AngleValue;
    attribute def DimensionlessValue;
}

standard library package ISQMechanics {
    attribute def PowerValue;
    attribute def ForceValue;
    attribute def PressureValue;
    attribute def TorqueValue;
    attribute def MomentumValue;
    attribute def FrequencyValue;
}

standard library package ISQThermodynamics {
    attribute def EnergyValue;
    attribute def HeatCapacityValue;
    attribute def ThermalConductivityValue;
}

standard library package ISQ {
    public import ISQBase::*;
    public import ISQMechanics::*;
    public import ISQThermodynamics::*;

    attribute def TemperatureDifferenceValue;
    attribute def TemperatureDifferenceUnit;
}
