// Loads model credentials from `.env` into `process.env` so that
// `@midscene/computer` can pick them up
// (MIDSCENE_MODEL_BASE_URL / MIDSCENE_MODEL_API_KEY / MIDSCENE_MODEL_NAME /
//  MIDSCENE_MODEL_FAMILY).
//
// Copy `.env.example` to `.env` and fill in your model configuration before
// running the tests.
import 'dotenv/config';
