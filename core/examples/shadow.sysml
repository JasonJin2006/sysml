package Left {
    part def Wheel {
        attribute spokes : Real;
    }
    part def Bike {
        part front : Wheel;
    }
}

package Right {
    part def Wheel {
        attribute diameter : Real;
    }
    part def Cart {
        part wheel : Wheel;
    }
}
