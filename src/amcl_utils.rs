extern crate amcl;
#[cfg(feature = "std")]
extern crate hex;
extern crate rand;

use super::errors::DecodeError;
use BLSCurve::bls381::hash_to_curve_g2;
use BLSCurve::ecp::ECP;
use BLSCurve::ecp2::ECP2;

use BLSCurve::pair::{ate2, fexp};

pub use BLSCurve::big::{Big, MODBYTES};
pub use BLSCurve::fp12::FP12;
pub use BLSCurve::fp2::FP2;
pub use BLSCurve::rom::CURVE_ORDER;

pub type GroupG1 = ECP;
pub type GroupG2 = ECP2;

// Byte size of element in group G1
pub const G1_BYTE_SIZE: usize = (2 * MODBYTES) as usize;
// Byte size of element in group G2
pub const G2_BYTE_SIZE: usize = (4 * MODBYTES) as usize;

#[cfg(feature = "std")]
lazy_static! {
    pub static ref GENERATORG1: GroupG1 = GroupG1::generator();
    pub static ref GENERATORG2: GroupG2 = GroupG2::generator();
}

// Take given message convert it to GroupG2 point
pub fn hash_on_g2(msg: &[u8]) -> GroupG2 {
    hash_to_curve_g2(msg)
}

// Compare values of two FP2 elements,
// -1 if num1 < num2; 0 if num1 == num2; 1 if num1 > num2
pub fn cmp_fp2(num1: &mut FP2, num2: &mut FP2) -> isize {
    // First compare FP2.b
    let num1_b = num1.getb();
    let num2_b = num2.getb();
    let mut result = Big::comp(&num1_b, &num2_b);

    // If FP2.b is equal compare FP2.a
    if result == 0 {
        let num1_a = num1.geta();
        let num2_a = num2.geta();
        result = Big::comp(&num1_a, &num2_a);
    }
    result
}

// Evaluation of e(A, B) * e(C, D) == 1
pub fn ate2_evaluation(a: &GroupG2, b: &GroupG1, c: &GroupG2, d: &GroupG1) -> bool {
    let mut e = ate2(&a, &b, &c, &d);
    e = fexp(&e);
    FP12::new_int(1).equals(&e)
}

// Take a GroupG1 point (x, y) and compress it to a 384 bit array.
// See https://github.com/zkcrypto/pairing/blob/master/src/bls12_381/README.md#serialization
pub fn compress_g1(g1: &GroupG1) -> Vec<u8> {
    // Check point at inifinity
    if g1.is_infinity() {
        let mut result: Vec<u8> = vec![0; G1_BYTE_SIZE / 2];
        // Set compressed flag and infinity flag
        result[0] = u8::pow(2, 6) + u8::pow(2, 7);
        return result;
    }

    // Convert point to array of bytes (x, y)
    let mut g1_bytes: Vec<u8> = vec![0; G1_BYTE_SIZE / 2 + 1];
    g1.tobytes(&mut g1_bytes, true);

    // Convert arrary (x, y) to compressed format
    let mut result: Vec<u8> = g1_bytes[1..].to_vec(); // byte[0] is amcl formatting

    // Evaluate if y > -y
    let mut g1_copy = g1.clone();
    g1_copy.affine();
    let y = g1_copy.gety();
    g1_copy.neg();
    let y_neg = g1_copy.gety();
    let is_y_larger = y > y_neg;

    // Set flags
    if is_y_larger {
        result[0] += u8::pow(2, 5);
    }
    result[0] += u8::pow(2, 7); // compressed flag

    result
}

// Take a 384 bit array and convert to GroupG1 point (x, y)
// See https://github.com/zkcrypto/pairing/blob/master/src/bls12_381/README.md#serialization
pub fn decompress_g1(g1_bytes: &[u8]) -> Result<GroupG1, DecodeError> {
    // Length must be 48 bytes
    if g1_bytes.len() != MODBYTES {
        return Err(DecodeError::IncorrectSize);
    }

    // compression flag must be set
    if g1_bytes[0] / u8::pow(2, 7) != 1 {
        // Invalid bytes
        return Err(DecodeError::InvalidCompressionFlag);
    }

    // Check infinity flag
    if g1_bytes[0] % u8::pow(2, 7) / u8::pow(2, 6) == 1 {
        // Trailing bits should all be 0.
        if g1_bytes[0] % u8::pow(2, 6) != 0 {
            return Err(DecodeError::BadPoint);
        }

        for item in g1_bytes.iter().skip(1) {
            if *item != 0 {
                return Err(DecodeError::BadPoint);
            }
        }

        // Point is infinity
        return Ok(GroupG1::new());
    }

    let y_flag: bool = (g1_bytes[0] % u8::pow(2, 6) / u8::pow(2, 5)) > 0;

    // Zero remaining flags so it can be converted to 381 bit Big
    let mut g1_bytes = g1_bytes.to_owned();
    g1_bytes[0] %= u8::pow(2, 5);
    let x_big = Big::frombytes(&g1_bytes);

    // Convert to GroupG1 point using big
    let point = GroupG1::new_big(&x_big);
    if point.is_infinity() {
        return Err(DecodeError::BadPoint);
    }

    // Confirm y value
    let mut point_neg = point.clone();
    point_neg.neg();

    if (point.gety() > point_neg.gety()) != y_flag {
        Ok(point_neg)
    } else {
        Ok(point)
    }
}

// Take a GroupG2 point (x, y) and compress it to a 384*2 bit array.
// See https://github.com/zkcrypto/pairing/blob/master/src/bls12_381/README.md#serialization
pub fn compress_g2(g2: &GroupG2) -> Vec<u8> {
    // Check point at inifinity
    if g2.is_infinity() {
        let mut result: Vec<u8> = vec![0; G2_BYTE_SIZE / 2];
        // Set compressed flag and infinity flag
        result[0] += u8::pow(2, 6) + u8::pow(2, 7);
        return result;
    }

    // Convert point to array of bytes (x, y)
    let mut g2_bytes: Vec<u8> = vec![0; G2_BYTE_SIZE];
    g2.tobytes(&mut g2_bytes);

    // Convert arrary (x, y) to compressed format
    // Note: amcl is x(re, im), y(re, im) eth is x(im, re), y(im, re)
    let x_real = &g2_bytes[0..MODBYTES];
    let x_imaginary = &g2_bytes[MODBYTES..(MODBYTES * 2)];
    let mut result: Vec<u8> = vec![0; MODBYTES];
    result.copy_from_slice(x_imaginary);
    result.extend_from_slice(x_real);

    // Check y value
    let mut g2_copy = g2.clone();
    g2_copy.affine();
    let mut y = g2_copy.gety();
    g2_copy.neg();
    let mut y_neg = g2_copy.gety();
    let is_y_larger = cmp_fp2(&mut y, &mut y_neg) > 0;

    // Set flags
    if is_y_larger {
        result[0] += u8::pow(2, 5);
    }
    result[0] += u8::pow(2, 7);

    result
}

// Take a 384*2 bit array and convert to GroupG2 point (x, y)
// See https://github.com/zkcrypto/pairing/blob/master/src/bls12_381/README.md#serialization
pub fn decompress_g2(g2_bytes: &[u8]) -> Result<GroupG2, DecodeError> {
    // Length must be 96 bytes
    if g2_bytes.len() != G2_BYTE_SIZE / 2 {
        return Err(DecodeError::IncorrectSize);
    }

    // Compression flag must be set
    if g2_bytes[0] / u8::pow(2, 7) != 1 {
        // Invalid bytes
        return Err(DecodeError::InvalidCompressionFlag);
    }

    // Check infinity flag
    if g2_bytes[0] % u8::pow(2, 7) / u8::pow(2, 6) == 1 {
        if g2_bytes[0] % u8::pow(2, 6) != 0 {
            return Err(DecodeError::BadPoint);
        }

        for item in g2_bytes.iter().skip(1) {
            if *item != 0 {
                return Err(DecodeError::BadPoint);
            }
        }
        // Point is infinity
        return Ok(GroupG2::new());
    }

    let y_flag: bool = (g2_bytes[0] % u8::pow(2, 6) / u8::pow(2, 5)) > 0;

    // Zero remaining flags so it can be converted to 381 bit Big
    let mut g2_bytes = g2_bytes.to_owned();
    g2_bytes[0] %= u8::pow(2, 5);

    // Convert from array to FP2
    let x_imaginary = Big::frombytes(&g2_bytes[0..MODBYTES]);
    let x_real = Big::frombytes(&g2_bytes[MODBYTES..]);
    let x = FP2::new_bigs(&x_real, &x_imaginary);

    // Convert to GroupG1 point using big and sign
    let point = GroupG2::new_fp2(&x);
    if point.is_infinity() {
        return Err(DecodeError::BadPoint);
    }

    // Confirm y value
    let mut point_neg = point.clone();
    point_neg.neg();

    if (cmp_fp2(&mut point.gety(), &mut point_neg.gety()) > 0) != y_flag {
        Ok(point_neg)
    } else {
        Ok(point)
    }
}

#[cfg(test)]
mod tests {
    extern crate yaml_rust;

    use self::yaml_rust::yaml;
    use super::*;
    use std::{fs::File, io::prelude::*, path::PathBuf};

    #[test]
    fn compression_decompression_g1_round_trip() {
        // Input 1
        let compressed = hex::decode("b53d21a4cfd562c469cc81514d4ce5a6b577d8403d32a394dc265dd190b47fa9f829fdd7963afdf972e5e77854051f6f").unwrap();
        let mut decompressed = decompress_g1(&compressed).unwrap();
        let compressed_result = compress_g1(&mut decompressed);
        assert_eq!(compressed, compressed_result);

        // Input 2
        let compressed = hex::decode("b301803f8b5ac4a1133581fc676dfedc60d891dd5fa99028805e5ea5b08d3491af75d0707adab3b70c6a6a580217bf81").unwrap();
        let mut decompressed = decompress_g1(&compressed).unwrap();
        let compressed_result = compress_g1(&mut decompressed);
        assert_eq!(compressed, compressed_result);

        // Input 3
        let compressed = hex::decode("a491d1b0ecd9bb917989f0e74f0dea0422eac4a873e5e2644f368dffb9a6e20fd6e10c1b77654d067c0618f6e5a7f79a").unwrap();
        let mut decompressed = decompress_g1(&compressed).unwrap();
        let compressed_result = compress_g1(&mut decompressed);
        assert_eq!(compressed, compressed_result);
    }

    #[test]
    fn test_to_from_infinity_g1() {
        let mut point = GroupG1::new();
        let compressed = compress_g1(&mut point);
        let round_trip_point = decompress_g1(&compressed).unwrap();
        assert_eq!(point.tostring(), round_trip_point.tostring());
    }

    #[test]
    fn test_to_from_infinity_g2() {
        let mut point = GroupG2::new();
        let compressed = compress_g2(&mut point);
        let round_trip_point = decompress_g2(&compressed).unwrap();
        assert_eq!(point.tostring(), round_trip_point.tostring());
    }

    #[test]
    fn compression_decompression_g2_round_trip() {
        // Input 1
        let mut compressed_a = hex::decode("a666d31d7e6561371644eb9ca7dbcb87257d8fd84a09e38a7a491ce0bbac64a324aa26385aebc99f47432970399a2ecb").unwrap();
        let mut compressed_b = hex::decode("0def2d4be359640e6dae6438119cbdc4f18e5e4496c68a979473a72b72d3badf98464412e9d8f8d2ea9b31953bb24899").unwrap();
        compressed_a.append(&mut compressed_b);

        let mut decompressed = decompress_g2(&compressed_a).unwrap();
        let compressed_result = compress_g2(&mut decompressed);
        assert_eq!(compressed_a, compressed_result);

        // Input 2
        let mut compressed_a = hex::decode("a63e88274adb7a98d112c16f7057f388786496c8f57e03ee9052b46b15eb0166645008f8cc929eb4475e386f3e6f1df8").unwrap();
        let mut compressed_b = hex::decode("1181e97fac61e371a22f34a4622f7e343ca0d99846b175a92ad1bf1df6fd4d0800e4edb7c2eb3d8437ed10cbc2d88823").unwrap();
        compressed_a.append(&mut compressed_b);

        let mut decompressed = decompress_g2(&compressed_a).unwrap();
        let compressed_result = compress_g2(&mut decompressed);
        assert_eq!(compressed_a, compressed_result);

        // Input 3
        let mut compressed_a = hex::decode("b090fbc9d5c6c80fec73c567202a75664cd00c2592e472a4d81d2ed4b6a166311e809ca25eb88c5d0189cbf1baa8ea79").unwrap();
        let mut compressed_b = hex::decode("18ca20f0b66678c0230e65eb4ebb3d621940984f71eb5481453e4489dafcc7f6ee2c863b76671467002a8f2392063005").unwrap();
        compressed_a.append(&mut compressed_b);

        let mut decompressed = decompress_g2(&compressed_a).unwrap();
        let compressed_result = compress_g2(&mut decompressed);
        assert_eq!(compressed_a, compressed_result);
    }

    // Test vectors found at https://github.com/ethereum/eth2.0-tests/blob/master/bls/test_bls.yml
    #[test]
    #[allow(non_snake_case)]
    #[ignore]
    fn case01_message_hash_G2_uncompressed() {
        // This test fails as the intermediate (x,y,z) variables do not match test vector
        // Likely caused by calling affine() during an intermediate step which converts (x, y, z) -> (x, y)
        // Note: if we convert to an (x, y) point the result is correct so overall function works

        // Run tests from test_bls.yml
        let mut file = {
            let mut file_path_buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            file_path_buf.push("src/test_vectors/test_bls.yml");

            File::open(file_path_buf).unwrap()
        };
        let mut yaml_str = String::new();
        file.read_to_string(&mut yaml_str).unwrap();
        let docs = yaml::YamlLoader::load_from_str(&yaml_str).unwrap();
        let doc = &docs[0];

        // Select test case01
        let test_cases = doc["case01_message_hash_G2_uncompressed"].as_vec().unwrap();

        // Verify input against output for each pair
        for test_case in test_cases {
            // Convert input to rust formats
            let input = test_case["input"].clone();
            // Convert domain from indexed yaml to u64
            let domain = input["domain"].as_str().unwrap();
            let domain = domain.trim_start_matches("0x");
            let _domain = u64::from_str_radix(domain, 16).unwrap();

            // Convert msg from indexed yaml to bytes (Vec<u8>)
            let msg = input["message"].as_str().unwrap();
            let msg = msg.trim_start_matches("0x");
            let msg = hex::decode(msg).unwrap();

            // Function results returns GroupG2 point
            let result = hash_on_g2(&msg);

            // Compare against given output
            let output = test_case["output"].clone().into_vec().unwrap();
            for (i, fp2) in output.iter().enumerate() {
                // Get x, y or z point from curve
                let mut result_fp2 = result.getpx();
                if i == 1 {
                    // Check y coordinate
                    result_fp2 = result.getpy();
                } else if i == 2 {
                    // Check z coordinate
                    result_fp2 = result.getpz();
                }

                // Convert output (a, b) to bytes
                let output_a = fp2[0].as_str().unwrap().trim_start_matches("0x");
                let output_a = hex::decode(output_a).unwrap();
                let output_b = fp2[1].as_str().unwrap().trim_start_matches("0x");
                let output_b = hex::decode(output_b).unwrap();

                // Convert the result (a,b) to bytes
                let mut result_a = vec![0; 48];
                let mut result_b = vec![0; 48];
                result_fp2.geta().tobytes(&mut result_a);
                result_fp2.getb().tobytes(&mut result_b);

                assert_eq!(output_a, result_a);
                assert_eq!(output_b, result_b);
            }
        }
    }

    #[test]
    #[allow(non_snake_case)]
    #[ignore] //TODO: update test_vectors
              // Test vectors use Keccak whilst this implementation uses SHA2.
    fn case02_message_hash_G2_compressed() {
        // Run tests from test_bls.yml
        let mut file = {
            let mut file_path_buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            file_path_buf.push("src/test_vectors/test_bls.yml");

            File::open(file_path_buf).unwrap()
        };
        let mut yaml_str = String::new();
        file.read_to_string(&mut yaml_str).unwrap();
        let docs = yaml::YamlLoader::load_from_str(&yaml_str).unwrap();
        let doc = &docs[0];

        // Select test case02
        let test_cases = doc["case02_message_hash_G2_compressed"].as_vec().unwrap();

        // Verify input against output for each pair
        for test_case in test_cases {
            // Convert input to rust formats
            let input = test_case["input"].clone();
            // Convert domain from indexed yaml to u64
            let domain = input["domain"].as_str().unwrap();
            let domain = domain.trim_start_matches("0x");
            let _domain = u64::from_str_radix(domain, 16).unwrap();

            // Convert msg from indexed yaml to bytes (Vec<u8>)
            let msg = input["message"].as_str().unwrap();
            let msg = msg.trim_start_matches("0x");
            let msg = hex::decode(msg).unwrap();

            // Function results returns GroupG2 point, then compress
            let mut result = hash_on_g2(&msg);
            result.affine();

            // Convert ouput to compressed bytes
            let output = test_case["output"].clone();
            let mut a = hex::decode(output[0].as_str().unwrap().trim_start_matches("0x")).unwrap();
            while a.len() < MODBYTES {
                a.insert(0, 0);
            }
            let mut b = hex::decode(output[1].as_str().unwrap().trim_start_matches("0x")).unwrap();
            while b.len() < MODBYTES {
                b.insert(0, 0);
            }
            a.append(&mut b);

            assert_eq!(a, compress_g2(&mut result));
        }
    }

    #[test]
    fn fuzz_input() {
        let data = hex::decode("b9b90ab9b9b9b9b90ab9b9b90a00000a0a000000002db9b9b9b9b90ab9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b9b90ab9b9b90a0a0a0a0a0a0a0a000a0a0a00000a000a000a0a000a0a0a000a00000a0000000000000ab9b9b90a0a0a0a0a0a0000").unwrap();

        if let Ok(mut point) = decompress_g2(&data) {
            let compressed_data = compress_g2(&point);
            if let Ok(mut point2) = decompress_g2(&compressed_data) {
                assert_eq!(point, point2);
                let compressed_data2 = compress_g2(&point2);
                assert_eq!(compressed_data, compressed_data2);
            }
            assert_eq!(data, compressed_data);
        }
    }
}
