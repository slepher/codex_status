import fs from 'node:fs';
import zlib from 'node:zlib';

const WIDTH = 200;
const HEIGHT = 200;
const COLORS = Object.freeze({
  white: Object.freeze([0xff, 0xff, 0xff]),
  black: Object.freeze([0x00, 0x00, 0x00]),
});
const OUTPUT = new URL('../artifacts/codex-quota-preview.png', import.meta.url);

const OUTER_FRAME = Object.freeze({ x: 0, y: 0, width: WIDTH, height: HEIGHT, thickness: 1 });
const METADATA_REGION = Object.freeze({ x: 5, y: 68, width: 190, height: 5 });
const FIVE_HOUR_REGION = Object.freeze({ x: 5, y: 77, width: 190, height: 20 });
const WEEK_REGION = Object.freeze({ x: 5, y: 103, width: 190, height: 20 });
const GLOBAL_REGION = Object.freeze({ x: 5, y: 127, width: 190, height: 5 });
const CONTENT_BANDS = Object.freeze([
  Object.freeze({ name: 'metadata', ...METADATA_REGION }),
  Object.freeze({ name: 'fiveHour', ...FIVE_HOUR_REGION }),
  Object.freeze({ name: 'week', ...WEEK_REGION }),
  Object.freeze({ name: 'global', ...GLOBAL_REGION }),
]);

const BAR_X = 54;
const BAR_OUTER_WIDTH = 102;
const BAR_OUTER_HEIGHT = 9;
const BAR_INTERIOR_X = BAR_X + 1;
const BAR_INTERIOR_WIDTH = 100;
const BAR_INTERIOR_Y_OFFSET = 1;
const BAR_INTERIOR_HEIGHT = BAR_OUTER_HEIGHT - 2;
const FIVE_HOUR_BAR_Y = 77;
const WEEK_BAR_Y = 103;

// Compact 4x5 glyphs keep every label hard-edged and readable at 200x200.
const FONT = Object.freeze({
  ' ': ['00', '00', '00', '00', '00'],
  '%': ['1100', '0010', '0100', '1000', '0011'],
  ':': ['00', '11', '00', '11', '00'],
  '0': ['0110', '1001', '1001', '1001', '0110'],
  '1': ['0100', '1100', '0100', '0100', '1110'],
  '2': ['1110', '0001', '0110', '1000', '1111'],
  '3': ['1110', '0001', '0110', '0001', '1110'],
  '4': ['1001', '1001', '1111', '0001', '0001'],
  '5': ['1111', '1000', '1110', '0001', '1110'],
  '6': ['0111', '1000', '1110', '1001', '0110'],
  '7': ['1111', '0001', '0010', '0100', '0100'],
  '8': ['0110', '1001', '0110', '1001', '0110'],
  '9': ['0110', '1001', '0111', '0001', '1110'],
  A: ['0110', '1001', '1111', '1001', '1001'],
  C: ['0111', '1000', '1000', '1000', '0111'],
  D: ['1110', '1001', '1001', '1001', '1110'],
  E: ['1111', '1000', '1110', '1000', '1111'],
  F: ['1111', '1000', '1110', '1000', '1000'],
  I: ['1111', '0110', '0110', '0110', '1111'],
  K: ['1001', '1010', '1100', '1010', '1001'],
  L: ['1000', '1000', '1000', '1000', '1111'],
  M: ['1001', '1111', '1111', '1001', '1001'],
  N: ['1001', '1101', '1011', '1001', '1001'],
  O: ['0110', '1001', '1001', '1001', '0110'],
  P: ['1110', '1001', '1110', '1000', '1000'],
  R: ['1110', '1001', '1110', '1010', '1001'],
  S: ['0111', '1000', '0110', '0001', '1110'],
  T: ['1111', '0110', '0110', '0110', '0110'],
  U: ['1001', '1001', '1001', '1001', '0110'],
  V: ['1001', '1001', '1001', '0110', '0110'],
  W: ['1001', '1001', '1111', '1111', '1001'],
  X: ['1001', '0110', '0110', '0110', '1001'],
  Y: ['1001', '1001', '0110', '0110', '0110'],
  H: ['1001', '1001', '1111', '1001', '1001'],
  '-': ['0000', '0000', '1111', '0000', '0000'],
});

// Fixed representative data keeps this preview honest about being simulated.
function createModel() {
  return Object.freeze({
    planName: 'PLUS',
    userName: 'DEMO',
    simulated: true,
    resetCount: 3,
    expiresOn: '2026-09-21',
    fiveHour: Object.freeze({ used: 11, leftPercent: 89, nextRefresh: '20:53' }),
    week: Object.freeze({ used: 2, leftPercent: 98, nextRefresh: 'SEP 14 15:53' }),
  });
}

function measureText(value, scale) {
  if (!Number.isInteger(scale) || scale < 1) {
    throw new Error('text scale must be a positive integer');
  }
  let width = 0;
  for (const character of value) {
    const glyph = FONT[character];
    if (!glyph) {
      throw new Error(`missing glyph: ${JSON.stringify(character)}`);
    }
    width += glyph[0].length + 1;
  }
  return value.length === 0 ? 0 : width * scale - scale;
}

function text(value, x, y, scale = 1) {
  return { type: 'text', value, x, y, scale, color: 'black' };
}

function centeredText(value, region, y, scale = 1) {
  const width = measureText(value, scale);
  return text(value, region.x + Math.floor((region.width - width) / 2), y, scale);
}

function rightAlignedText(value, region, y, scale = 1) {
  return text(value, region.x + region.width - measureText(value, scale), y, scale);
}

function bar(y, value) {
  return {
    type: 'progress',
    x: BAR_X,
    y,
    width: BAR_OUTER_WIDTH,
    height: BAR_OUTER_HEIGHT,
    interiorX: BAR_INTERIOR_X,
    interiorY: y + BAR_INTERIOR_Y_OFFSET,
    interiorWidth: BAR_INTERIOR_WIDTH,
    interiorHeight: BAR_INTERIOR_HEIGHT,
    value,
  };
}

function validateModel(model) {
  if (model.planName !== 'PLUS' || model.userName !== 'DEMO' || model.simulated !== true) {
    throw new Error('preview model must remain the fixed simulated account');
  }
  if (Object.hasOwn(model, 'nextRefresh')) {
    throw new Error('next refresh times must belong to their quota windows');
  }
  if (model.fiveHour.used + model.fiveHour.leftPercent !== 100
    || model.week.used + model.week.leftPercent !== 100) {
    throw new Error('quota windows must each total 100');
  }
  if (Object.hasOwn(model.fiveHour, 'resetCount') || Object.hasOwn(model.fiveHour, 'expiresOn')
    || Object.hasOwn(model.week, 'resetCount') || Object.hasOwn(model.week, 'expiresOn')) {
    throw new Error('reset count and expiration must be global model properties');
  }
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(model.fiveHour.nextRefresh)) {
    throw new Error('5H next refresh must use 24-hour HH:MM format');
  }
  if (!/^(JAN|FEB|MAR|APR|MAY|JUN|JUL|AUG|SEP|OCT|NOV|DEC) (0[1-9]|[12]\d|3[01]) ([01]\d|2[0-3]):[0-5]\d$/.test(model.week.nextRefresh)) {
    throw new Error('WEEK next refresh must use MMM DD HH:MM format');
  }
  if (!Number.isInteger(model.resetCount) || model.resetCount < 0) {
    throw new Error('global reset count must be a non-negative integer');
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(model.expiresOn)) {
    throw new Error('global expiration must be an ISO date');
  }
}

function createLayout(model) {
  validateModel(model);
  const metadataCommands = Object.freeze([
    text(`PLAN ${model.planName}`, METADATA_REGION.x, 68),
    rightAlignedText(`USER ${model.userName}`, METADATA_REGION, 68),
  ]);
  const fiveHourCommands = Object.freeze([
    text('5H', FIVE_HOUR_REGION.x, 79, 2),
    bar(FIVE_HOUR_BAR_Y, model.fiveHour.leftPercent),
    centeredText(`${model.fiveHour.leftPercent}%`, { x: 162, y: FIVE_HOUR_REGION.y, width: 33, height: FIVE_HOUR_REGION.height }, 79, 2),
    centeredText(`NEXT ${model.fiveHour.nextRefresh}`, { x: BAR_X, y: 92, width: BAR_OUTER_WIDTH, height: 5 }, 92),
  ]);
  const weekCommands = Object.freeze([
    text('WEEK', WEEK_REGION.x, 105, 2),
    bar(WEEK_BAR_Y, model.week.leftPercent),
    centeredText(`${model.week.leftPercent}%`, { x: 162, y: WEEK_REGION.y, width: 33, height: WEEK_REGION.height }, 105, 2),
    centeredText(`NEXT ${model.week.nextRefresh}`, { x: BAR_X, y: 118, width: BAR_OUTER_WIDTH, height: 5 }, 118),
  ]);
  const globalCommands = Object.freeze([
    text(`RESET ${model.resetCount}`, GLOBAL_REGION.x, 127),
    rightAlignedText(`EXP ${model.expiresOn}`, GLOBAL_REGION, 127),
  ]);
  const commandGroups = Object.freeze({
    metadataCommands,
    fiveHourCommands,
    weekCommands,
    globalCommands,
  });
  const commands = [
    ...metadataCommands,
    ...fiveHourCommands,
    ...weekCommands,
    ...globalCommands,
    { type: 'frame', ...OUTER_FRAME },
  ];
  validateLayout(commands, commandGroups);
  return commands;
}

function commandBounds(command) {
  if (command.type === 'text') {
    return { x: command.x, y: command.y, width: measureText(command.value, command.scale), height: 5 * command.scale };
  }
  return { x: command.x, y: command.y, width: command.width, height: command.height };
}

function rectanglesOverlap(first, second) {
  return first.x < second.x + second.width
    && second.x < first.x + first.width
    && first.y < second.y + second.height
    && second.y < first.y + first.height;
}

function validateCommandGroups(commandGroups, commands) {
  const expectedGroupNames = [
    'metadataCommands',
    'fiveHourCommands',
    'weekCommands',
    'globalCommands',
  ];
  const actualGroupNames = Object.keys(commandGroups);
  if (actualGroupNames.length !== expectedGroupNames.length
    || expectedGroupNames.some((name, index) => actualGroupNames[index] !== name)) {
    throw new Error('layout must contain exactly four ordered command groups');
  }
  const groupedCommands = [];
  for (const groupName of expectedGroupNames) {
    const group = commandGroups[groupName];
    if (!Array.isArray(group) || group.length === 0) {
      throw new Error(`${groupName} must be a non-empty command group`);
    }
    const bandName = groupName.slice(0, -'Commands'.length);
    const band = CONTENT_BANDS.find((candidate) => candidate.name === bandName);
    for (const command of group) {
      if (command.type === 'frame') {
        throw new Error(`${groupName} cannot contain the frame command`);
      }
      const bounds = commandBounds(command);
      if (!band || bounds.x < band.x || bounds.x + bounds.width > band.x + band.width
        || bounds.y < band.y || bounds.y + bounds.height > band.y + band.height) {
        throw new Error(`${groupName} command is outside its content band`);
      }
      groupedCommands.push(command);
    }
  }
  if (new Set(groupedCommands).size !== groupedCommands.length) {
    throw new Error('content commands must belong to exactly one command group');
  }
  const groupText = (groupName) => commandGroups[groupName]
    .filter((command) => command.type === 'text')
    .map((command) => command.value)
    .join('\n');
  const allText = expectedGroupNames.map(groupText).join('\n');
  if (/CODEX STATUS|SIMULATED|USED|LEFT/.test(allText)) {
    throw new Error('removed visible labels must not occur in draw commands');
  }
  const fiveHourText = groupText('fiveHourCommands');
  const weekText = groupText('weekCommands');
  const globalText = groupText('globalCommands');
  if ((fiveHourText.match(/NEXT/g) || []).length !== 1
    || !fiveHourText.includes('NEXT 20:53')
    || (weekText.match(/NEXT/g) || []).length !== 1
    || !weekText.includes('NEXT SEP 14 15:53')
    || globalText.includes('NEXT')) {
    throw new Error('NEXT text must be unique to its quota window command group');
  }
  if ((globalText.match(/RESET/g) || []).length !== 1
    || (globalText.match(/EXP/g) || []).length !== 1
    || fiveHourText.includes('RESET') || fiveHourText.includes('EXP')
    || weekText.includes('RESET') || weekText.includes('EXP')) {
    throw new Error('RESET and EXP text must be unique to globalCommands');
  }
  const contentCommands = commands.filter((command) => command.type !== 'frame');
  if (contentCommands.length !== groupedCommands.length
    || contentCommands.some((command) => !groupedCommands.includes(command))) {
    throw new Error('command groups must cover every non-frame command exactly once');
  }
}

function validateContentCommandBands(commands) {
  const contentCommands = commands.filter((command) => command.type !== 'frame');
  for (const command of contentCommands) {
    const bounds = commandBounds(command);
    const band = CONTENT_BANDS.find((candidate) => (
      bounds.x >= candidate.x
      && bounds.x + bounds.width <= candidate.x + candidate.width
      && bounds.y >= candidate.y
      && bounds.y + bounds.height <= candidate.y + candidate.height
    ));
    if (!band) {
      throw new Error(`content command is outside its top-aligned band: ${command.type}`);
    }
  }
  for (let firstIndex = 0; firstIndex < contentCommands.length; firstIndex += 1) {
    const first = commandBounds(contentCommands[firstIndex]);
    for (let secondIndex = firstIndex + 1; secondIndex < contentCommands.length; secondIndex += 1) {
      const second = commandBounds(contentCommands[secondIndex]);
      if (rectanglesOverlap(first, second)) {
        throw new Error(`content command rectangles overlap: ${firstIndex}/${secondIndex}`);
      }
    }
  }
}

function validateLayout(commands, commandGroups) {
  for (const command of commands) {
    const bounds = commandBounds(command);
    if (![bounds.x, bounds.y, bounds.width, bounds.height].every(Number.isInteger)
      || bounds.width < 0 || bounds.height < 0
      || bounds.x < 0 || bounds.y < 0
      || bounds.x + bounds.width > WIDTH || bounds.y + bounds.height > HEIGHT) {
      throw new Error(`layout command exceeds ${WIDTH}x${HEIGHT}`);
    }
    if (command.type === 'text' && !COLORS[command.color]) {
      throw new Error(`unknown text color: ${command.color}`);
    }
    if (command.type === 'progress'
      && (command.width !== BAR_OUTER_WIDTH
        || command.interiorWidth !== BAR_INTERIOR_WIDTH
        || command.x !== BAR_X
        || command.interiorX !== BAR_INTERIOR_X
        || command.interiorHeight !== BAR_INTERIOR_HEIGHT
        || command.value < 0 || command.value > 100)) {
      throw new Error('invalid progress bar geometry');
    }
  }
  validateCommandGroups(commandGroups, commands);
  validateContentCommandBands(commands);
  const contentCommands = commands.filter((command) => command.type !== 'frame');
  const contentMinY = Math.min(...contentCommands.map((command) => commandBounds(command).y));
  const contentMaxY = Math.max(...contentCommands.map((command) => {
    const bounds = commandBounds(command);
    return bounds.y + bounds.height - 1;
  }));
  if (contentMinY !== 68 || contentMaxY !== 131) {
    throw new Error(`content command range must be y68..131: ${contentMinY}..${contentMaxY}`);
  }
}

function createRaster() {
  return { width: WIDTH, height: HEIGHT, pixels: Buffer.alloc(WIDTH * HEIGHT * 3, 0xff) };
}

function paintPixel(raster, x, y, color) {
  const [red, green, blue] = COLORS[color];
  const offset = (y * raster.width + x) * 3;
  raster.pixels[offset] = red;
  raster.pixels[offset + 1] = green;
  raster.pixels[offset + 2] = blue;
}

function fillRect(raster, x, y, width, height, color) {
  for (let row = y; row < y + height; row += 1) {
    for (let column = x; column < x + width; column += 1) {
      paintPixel(raster, column, row, color);
    }
  }
}

function drawFrame(raster, frame) {
  fillRect(raster, frame.x, frame.y, frame.width, frame.thickness, 'black');
  fillRect(raster, frame.x, frame.y + frame.height - frame.thickness, frame.width, frame.thickness, 'black');
  fillRect(raster, frame.x, frame.y, frame.thickness, frame.height, 'black');
  fillRect(raster, frame.x + frame.width - frame.thickness, frame.y, frame.thickness, frame.height, 'black');
}

function drawText(raster, command) {
  const { value, x, y, scale, color } = command;
  let cursor = x;
  for (const character of value) {
    const glyph = FONT[character];
    for (let row = 0; row < glyph.length; row += 1) {
      for (let column = 0; column < glyph[row].length; column += 1) {
        if (glyph[row][column] === '1') {
          fillRect(raster, cursor + column * scale, y + row * scale, scale, scale, color);
        }
      }
    }
    cursor += (glyph[0].length + 1) * scale;
  }
}

function drawProgress(raster, command) {
  fillRect(raster, command.x, command.y, command.width, command.height, 'black');
  fillRect(raster, command.interiorX, command.interiorY, command.interiorWidth, command.interiorHeight, 'white');
  const filledWidth = Math.floor(command.interiorWidth * command.value / 100);
  fillRect(raster, command.interiorX, command.interiorY, filledWidth, command.interiorHeight, 'black');
}

function rasterize(commands) {
  const raster = createRaster();
  for (const command of commands) {
    if (command.type === 'frame') {
      drawFrame(raster, command);
    } else if (command.type === 'progress') {
      drawProgress(raster, command);
    } else if (command.type === 'text') {
      drawText(raster, command);
    } else {
      throw new Error(`unknown raster command: ${command.type}`);
    }
  }
  validatePalette(raster.pixels);
  validateRasterContent(raster);
  return raster;
}

function validatePalette(pixels) {
  const allowed = new Set(Object.values(COLORS).map((rgb) => rgb.join(',')));
  for (let offset = 0; offset < pixels.length; offset += 3) {
    const key = `${pixels[offset]},${pixels[offset + 1]},${pixels[offset + 2]}`;
    if (!allowed.has(key)) {
      throw new Error(`pixel outside the two-color palette at byte ${offset}`);
    }
  }
}

function isBlackPixel(raster, x, y) {
  const offset = (y * raster.width + x) * 3;
  return raster.pixels[offset] === 0x00
    && raster.pixels[offset + 1] === 0x00
    && raster.pixels[offset + 2] === 0x00;
}

function blackBoundsForBand(raster, band) {
  let minX = WIDTH;
  let minY = HEIGHT;
  let maxX = -1;
  let maxY = -1;
  for (let y = band.y; y < band.y + band.height; y += 1) {
    for (let x = 1; x < WIDTH - 1; x += 1) {
      if (!isBlackPixel(raster, x, y)) {
        continue;
      }
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }
  return maxX < 0 ? null : { minX, minY, maxX, maxY };
}

function validateRasterContent(raster) {
  const bandBounds = CONTENT_BANDS.map((band) => blackBoundsForBand(raster, band));
  if (bandBounds.some((bounds) => bounds === null)) {
    throw new Error('every centered content band must contain black pixels');
  }
  for (let y = 1; y < HEIGHT - 1; y += 1) {
    for (let x = 1; x < WIDTH - 1; x += 1) {
      if (isBlackPixel(raster, x, y)
        && !CONTENT_BANDS.some((band) => y >= band.y && y < band.y + band.height)) {
        throw new Error(`non-frame content is outside the declared bands at y=${y}`);
      }
    }
  }
  const expectedBandY = [
    [68, 72],
    [77, 96],
    [103, 122],
    [127, 131],
  ];
  for (let index = 0; index < bandBounds.length; index += 1) {
    const [expectedMinY, expectedMaxY] = expectedBandY[index];
    const bounds = bandBounds[index];
    if (bounds.minY !== expectedMinY || bounds.maxY !== expectedMaxY) {
      throw new Error(`content band ${CONTENT_BANDS[index].name} must occupy y${expectedMinY}..${expectedMaxY}, got y${bounds.minY}..${bounds.maxY}`);
    }
  }
  const contentMinY = Math.min(...bandBounds.map((bounds) => bounds.minY));
  const contentMaxY = Math.max(...bandBounds.map((bounds) => bounds.maxY));
  if (contentMinY !== 68 || contentMaxY !== 131) {
    throw new Error(`raster content must occupy y68..131, got y${contentMinY}..${contentMaxY}`);
  }
  let upperInteriorBlack = 0;
  for (let y = 1; y <= 67; y += 1) {
    for (let x = 1; x <= 198; x += 1) {
      if (isBlackPixel(raster, x, y)) {
        upperInteriorBlack += 1;
      }
    }
  }
  let lowerInteriorBlack = 0;
  for (let y = 132; y <= 198; y += 1) {
    for (let x = 1; x <= 198; x += 1) {
      if (isBlackPixel(raster, x, y)) {
        lowerInteriorBlack += 1;
      }
    }
  }
  if (upperInteriorBlack !== 0 || lowerInteriorBlack !== 0) {
    throw new Error(`center interior must remain blank, found upper=${upperInteriorBlack}, lower=${lowerInteriorBlack}`);
  }
  const expectedGaps = [4, 6, 4];
  for (let index = 1; index < bandBounds.length; index += 1) {
    const gap = bandBounds[index].minY - bandBounds[index - 1].maxY - 1;
    if (gap !== expectedGaps[index - 1]) {
      throw new Error(`content band gap ${index - 1}/${index} is ${gap}, expected ${expectedGaps[index - 1]}`);
    }
  }
}

function crc32(data) {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 1) === 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, 'ascii');
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
}

function encodePng(raster) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(raster.width, 0);
  header.writeUInt32BE(raster.height, 4);
  header[8] = 8;
  header[9] = 2;
  const scanlineLength = raster.width * 3;
  const scanlines = Buffer.alloc(raster.height * (scanlineLength + 1));
  for (let row = 0; row < raster.height; row += 1) {
    const scanlineOffset = row * (scanlineLength + 1);
    scanlines[scanlineOffset] = 0;
    raster.pixels.copy(scanlines, scanlineOffset + 1, row * scanlineLength, (row + 1) * scanlineLength);
  }
  const compressed = zlib.deflateSync(scanlines, { level: 9, strategy: zlib.constants.Z_FIXED });
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', compressed),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function main() {
  const model = createModel();
  const layout = createLayout(model);
  const raster = rasterize(layout);
  const png = encodePng(raster);
  fs.mkdirSync(new URL('../artifacts/', import.meta.url), { recursive: true });
  fs.writeFileSync(OUTPUT, png);
  console.log('Generated artifacts/codex-quota-preview.png (200x200 RGB monochrome PNG)');
}

try {
  main();
} catch (error) {
  console.error(`generate-preview: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
}
