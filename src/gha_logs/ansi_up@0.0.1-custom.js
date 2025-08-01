// SPDX-License-Identifier: MIT
/**
 * https://github.com/drudru/ansi_up
 *
 * (The MIT License)
 *
 * Copyright (c) 2011 github.com/drudru
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * 'Software'), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED 'AS IS', WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

"use strict";
var __makeTemplateObject = (this && this.__makeTemplateObject) || function (cooked, raw) {
    if (Object.defineProperty) { Object.defineProperty(cooked, "raw", { value: raw }); } else { cooked.raw = raw; }
    return cooked;
};
var PacketKind;
(function (PacketKind) {
    PacketKind[PacketKind["EOS"] = 0] = "EOS";
    PacketKind[PacketKind["Text"] = 1] = "Text";
    PacketKind[PacketKind["Incomplete"] = 2] = "Incomplete";
    PacketKind[PacketKind["ESC"] = 3] = "ESC";
    PacketKind[PacketKind["Unknown"] = 4] = "Unknown";
    PacketKind[PacketKind["SGR"] = 5] = "SGR";
    PacketKind[PacketKind["OSCURL"] = 6] = "OSCURL";
})(PacketKind || (PacketKind = {}));
export class AnsiUp {
    constructor() {
        this.VERSION = "6.0.6";
        this.setup_palettes();
        this._use_classes = false;
        this.bold = false;
        this.faint = false;
        this.italic = false;
        this.underline = false;
        this.fg = this.bg = null;
        this._buffer = '';
        this._url_allowlist = { 'http': 1, 'https': 1 };
        this._escape_html = true;
        this.boldStyle = 'font-weight:bold';
        this.faintStyle = 'opacity:0.7';
        this.italicStyle = 'font-style:italic';
        this.underlineStyle = 'text-decoration:underline';
    }
    set use_classes(arg) {
        this._use_classes = arg;
    }
    get use_classes() {
        return this._use_classes;
    }
    set url_allowlist(arg) {
        this._url_allowlist = arg;
    }
    get url_allowlist() {
        return this._url_allowlist;
    }
    set escape_html(arg) {
        this._escape_html = arg;
    }
    get escape_html() {
        return this._escape_html;
    }
    set boldStyle(arg) { this._boldStyle = arg; }
    get boldStyle() { return this._boldStyle; }
    set faintStyle(arg) { this._faintStyle = arg; }
    get faintStyle() { return this._faintStyle; }
    set italicStyle(arg) { this._italicStyle = arg; }
    get italicStyle() { return this._italicStyle; }
    set underlineStyle(arg) { this._underlineStyle = arg; }
    get underlineStyle() { return this._underlineStyle; }
    setup_palettes() {
        this.ansi_colors =
            [
                [
                    { rgb: [0, 0, 0], class_name: "ansi-black" },
                    { rgb: [187, 0, 0], class_name: "ansi-red" },
                    { rgb: [0, 187, 0], class_name: "ansi-green" },
                    { rgb: [187, 187, 0], class_name: "ansi-yellow" },
                    { rgb: [0, 0, 187], class_name: "ansi-blue" },
                    { rgb: [187, 0, 187], class_name: "ansi-magenta" },
                    { rgb: [0, 187, 187], class_name: "ansi-cyan" },
                    { rgb: [255, 255, 255], class_name: "ansi-white" }
                ],
                [
                    { rgb: [85, 85, 85], class_name: "ansi-bright-black" },
                    { rgb: [255, 85, 85], class_name: "ansi-bright-red" },
                    { rgb: [0, 255, 0], class_name: "ansi-bright-green" },
                    { rgb: [255, 255, 85], class_name: "ansi-bright-yellow" },
                    { rgb: [85, 85, 255], class_name: "ansi-bright-blue" },
                    { rgb: [255, 85, 255], class_name: "ansi-bright-magenta" },
                    { rgb: [85, 255, 255], class_name: "ansi-bright-cyan" },
                    { rgb: [255, 255, 255], class_name: "ansi-bright-white" }
                ]
            ];
        this.palette_256 = [];
        this.ansi_colors.forEach(palette => {
            palette.forEach(rec => {
                this.palette_256.push(rec);
            });
        });
        let levels = [0, 95, 135, 175, 215, 255];
        for (let r = 0; r < 6; ++r) {
            for (let g = 0; g < 6; ++g) {
                for (let b = 0; b < 6; ++b) {
                    let col = { rgb: [levels[r], levels[g], levels[b]], class_name: 'truecolor' };
                    this.palette_256.push(col);
                }
            }
        }
        let grey_level = 8;
        for (let i = 0; i < 24; ++i, grey_level += 10) {
            let gry = { rgb: [grey_level, grey_level, grey_level], class_name: 'truecolor' };
            this.palette_256.push(gry);
        }
    }
    escape_txt_for_html(txt) {
        if (!this._escape_html)
            return txt;
        return txt.replace(/[&<>"']/gm, (str) => {
            if (str === "&")
                return "&amp;";
            if (str === "<")
                return "&lt;";
            if (str === ">")
                return "&gt;";
            if (str === "\"")
                return "&quot;";
            if (str === "'")
                return "&#x27;";
        });
    }
    append_buffer(txt) {
        var str = this._buffer + txt;
        this._buffer = str;
    }
    get_next_packet() {
        var pkt = {
            kind: PacketKind.EOS,
            text: '',
            url: ''
        };
        var len = this._buffer.length;
        if (len == 0)
            return pkt;
        var pos = this._buffer.indexOf("\x1B");
        if (pos == -1) {
            pkt.kind = PacketKind.Text;
            pkt.text = this._buffer;
            this._buffer = '';
            return pkt;
        }
        if (pos > 0) {
            pkt.kind = PacketKind.Text;
            pkt.text = this._buffer.slice(0, pos);
            this._buffer = this._buffer.slice(pos);
            return pkt;
        }
        if (pos == 0) {
            if (len < 3) {
                pkt.kind = PacketKind.Incomplete;
                return pkt;
            }
            var next_char = this._buffer.charAt(1);
            if ((next_char != '[') && (next_char != ']') && (next_char != '(')) {
                pkt.kind = PacketKind.ESC;
                pkt.text = this._buffer.slice(0, 1);
                this._buffer = this._buffer.slice(1);
                return pkt;
            }
            if (next_char == '[') {
                if (!this._csi_regex) {
                    this._csi_regex = rgx(templateObject_1 || (templateObject_1 = __makeTemplateObject(["\n                        ^                           # beginning of line\n                                                    #\n                                                    # First attempt\n                        (?:                         # legal sequence\n                          \u001B[                      # CSI\n                          ([<-?]?)              # private-mode char\n                          ([d;]*)                    # any digits or semicolons\n                          ([ -/]?               # an intermediate modifier\n                          [@-~])                # the command\n                        )\n                        |                           # alternate (second attempt)\n                        (?:                         # illegal sequence\n                          \u001B[                      # CSI\n                          [ -~]*                # anything legal\n                          ([\0-\u001F:])              # anything illegal\n                        )\n                    "], ["\n                        ^                           # beginning of line\n                                                    #\n                                                    # First attempt\n                        (?:                         # legal sequence\n                          \\x1b\\[                      # CSI\n                          ([\\x3c-\\x3f]?)              # private-mode char\n                          ([\\d;]*)                    # any digits or semicolons\n                          ([\\x20-\\x2f]?               # an intermediate modifier\n                          [\\x40-\\x7e])                # the command\n                        )\n                        |                           # alternate (second attempt)\n                        (?:                         # illegal sequence\n                          \\x1b\\[                      # CSI\n                          [\\x20-\\x7e]*                # anything legal\n                          ([\\x00-\\x1f:])              # anything illegal\n                        )\n                    "])));
                }
                let match = this._buffer.match(this._csi_regex);
                if (match === null) {
                    pkt.kind = PacketKind.Incomplete;
                    return pkt;
                }
                if (match[4]) {
                    pkt.kind = PacketKind.ESC;
                    pkt.text = this._buffer.slice(0, 1);
                    this._buffer = this._buffer.slice(1);
                    return pkt;
                }
                if ((match[1] != '') || (match[3] != 'm'))
                    pkt.kind = PacketKind.Unknown;
                else
                    pkt.kind = PacketKind.SGR;
                pkt.text = match[2];
                var rpos = match[0].length;
                this._buffer = this._buffer.slice(rpos);
                return pkt;
            }
            else if (next_char == ']') {
                if (len < 4) {
                    pkt.kind = PacketKind.Incomplete;
                    return pkt;
                }
                if ((this._buffer.charAt(2) != '8')
                    || (this._buffer.charAt(3) != ';')) {
                    pkt.kind = PacketKind.ESC;
                    pkt.text = this._buffer.slice(0, 1);
                    this._buffer = this._buffer.slice(1);
                    return pkt;
                }
                if (!this._osc_st) {
                    this._osc_st = rgxG(templateObject_2 || (templateObject_2 = __makeTemplateObject(["\n                        (?:                         # legal sequence\n                          (\u001B\\)                    # ESC                           |                           # alternate\n                          (\u0007)                      # BEL (what xterm did)\n                        )\n                        |                           # alternate (second attempt)\n                        (                           # illegal sequence\n                          [\0-\u0006]                 # anything illegal\n                          |                           # alternate\n                          [\b-\u001A]                 # anything illegal\n                          |                           # alternate\n                          [\u001C-\u001F]                 # anything illegal\n                        )\n                    "], ["\n                        (?:                         # legal sequence\n                          (\\x1b\\\\)                    # ESC \\\n                          |                           # alternate\n                          (\\x07)                      # BEL (what xterm did)\n                        )\n                        |                           # alternate (second attempt)\n                        (                           # illegal sequence\n                          [\\x00-\\x06]                 # anything illegal\n                          |                           # alternate\n                          [\\x08-\\x1a]                 # anything illegal\n                          |                           # alternate\n                          [\\x1c-\\x1f]                 # anything illegal\n                        )\n                    "])));
                }
                this._osc_st.lastIndex = 0;
                {
                    let match = this._osc_st.exec(this._buffer);
                    if (match === null) {
                        pkt.kind = PacketKind.Incomplete;
                        return pkt;
                    }
                    if (match[3]) {
                        pkt.kind = PacketKind.ESC;
                        pkt.text = this._buffer.slice(0, 1);
                        this._buffer = this._buffer.slice(1);
                        return pkt;
                    }
                }
                {
                    let match = this._osc_st.exec(this._buffer);
                    if (match === null) {
                        pkt.kind = PacketKind.Incomplete;
                        return pkt;
                    }
                    if (match[3]) {
                        pkt.kind = PacketKind.ESC;
                        pkt.text = this._buffer.slice(0, 1);
                        this._buffer = this._buffer.slice(1);
                        return pkt;
                    }
                }
                if (!this._osc_regex) {
                    this._osc_regex = rgx(templateObject_3 || (templateObject_3 = __makeTemplateObject(["\n                        ^                           # beginning of line\n                                                    #\n                        \u001B]8;                    # OSC Hyperlink\n                        [ -:<-~]*       # params (excluding ;)\n                        ;                           # end of params\n                        ([!-~]{0,512})        # URL capture\n                        (?:                         # ST\n                          (?:\u001B\\)                  # ESC                           |                           # alternate\n                          (?:\u0007)                    # BEL (what xterm did)\n                        )\n                        ([ -~]+)              # TEXT capture\n                        \u001B]8;;                   # OSC Hyperlink End\n                        (?:                         # ST\n                          (?:\u001B\\)                  # ESC                           |                           # alternate\n                          (?:\u0007)                    # BEL (what xterm did)\n                        )\n                    "], ["\n                        ^                           # beginning of line\n                                                    #\n                        \\x1b\\]8;                    # OSC Hyperlink\n                        [\\x20-\\x3a\\x3c-\\x7e]*       # params (excluding ;)\n                        ;                           # end of params\n                        ([\\x21-\\x7e]{0,512})        # URL capture\n                        (?:                         # ST\n                          (?:\\x1b\\\\)                  # ESC \\\n                          |                           # alternate\n                          (?:\\x07)                    # BEL (what xterm did)\n                        )\n                        ([\\x20-\\x7e]+)              # TEXT capture\n                        \\x1b\\]8;;                   # OSC Hyperlink End\n                        (?:                         # ST\n                          (?:\\x1b\\\\)                  # ESC \\\n                          |                           # alternate\n                          (?:\\x07)                    # BEL (what xterm did)\n                        )\n                    "])));
                }
                let match = this._buffer.match(this._osc_regex);
                if (match === null) {
                    pkt.kind = PacketKind.ESC;
                    pkt.text = this._buffer.slice(0, 1);
                    this._buffer = this._buffer.slice(1);
                    return pkt;
                }
                pkt.kind = PacketKind.OSCURL;
                pkt.url = match[1];
                pkt.text = match[2];
                var rpos = match[0].length;
                this._buffer = this._buffer.slice(rpos);
                return pkt;
            }
            else if (next_char == '(') {
                pkt.kind = PacketKind.Unknown;
                this._buffer = this._buffer.slice(3);
                return pkt;
            }
        }
    }
    ansi_to_html(txt) {
        this.append_buffer(txt);
        var blocks = [];
        while (true) {
            var packet = this.get_next_packet();
            if ((packet.kind == PacketKind.EOS)
                || (packet.kind == PacketKind.Incomplete))
                break;
            if ((packet.kind == PacketKind.ESC)
                || (packet.kind == PacketKind.Unknown))
                continue;
            if (packet.kind == PacketKind.Text)
                blocks.push(this.transform_to_html(this.with_state(packet)));
            else if (packet.kind == PacketKind.SGR)
                this.process_ansi(packet);
            else if (packet.kind == PacketKind.OSCURL)
                blocks.push(this.process_hyperlink(packet));
        }
        return blocks.join("");
    }
    with_state(pkt) {
        return { bold: this.bold, faint: this.faint, italic: this.italic, underline: this.underline, fg: this.fg, bg: this.bg, text: pkt.text };
    }
    process_ansi(pkt) {
        let sgr_cmds = pkt.text.split(';');
        while (sgr_cmds.length > 0) {
            let sgr_cmd_str = sgr_cmds.shift();
            let num = parseInt(sgr_cmd_str, 10);
            if (isNaN(num) || num === 0) {
                this.fg = null;
                this.bg = null;
                this.bold = false;
                this.faint = false;
                this.italic = false;
                this.underline = false;
            }
            else if (num === 1) {
                this.bold = true;
            }
            else if (num === 2) {
                this.faint = true;
            }
            else if (num === 3) {
                this.italic = true;
            }
            else if (num === 4) {
                this.underline = true;
            }
            else if (num === 21) {
                this.bold = false;
            }
            else if (num === 22) {
                this.faint = false;
                this.bold = false;
            }
            else if (num === 23) {
                this.italic = false;
            }
            else if (num === 24) {
                this.underline = false;
            }
            else if (num === 39) {
                this.fg = null;
            }
            else if (num === 49) {
                this.bg = null;
            }
            else if ((num >= 30) && (num < 38)) {
                this.fg = this.ansi_colors[0][(num - 30)];
            }
            else if ((num >= 40) && (num < 48)) {
                this.bg = this.ansi_colors[0][(num - 40)];
            }
            else if ((num >= 90) && (num < 98)) {
                this.fg = this.ansi_colors[1][(num - 90)];
            }
            else if ((num >= 100) && (num < 108)) {
                this.bg = this.ansi_colors[1][(num - 100)];
            }
            else if (num === 38 || num === 48) {
                if (sgr_cmds.length > 0) {
                    let is_foreground = (num === 38);
                    let mode_cmd = sgr_cmds.shift();
                    if (mode_cmd === '5' && sgr_cmds.length > 0) {
                        let palette_index = parseInt(sgr_cmds.shift(), 10);
                        if (palette_index >= 0 && palette_index <= 255) {
                            if (is_foreground)
                                this.fg = this.palette_256[palette_index];
                            else
                                this.bg = this.palette_256[palette_index];
                        }
                    }
                    if (mode_cmd === '2' && sgr_cmds.length > 2) {
                        let r = parseInt(sgr_cmds.shift(), 10);
                        let g = parseInt(sgr_cmds.shift(), 10);
                        let b = parseInt(sgr_cmds.shift(), 10);
                        if ((r >= 0 && r <= 255) && (g >= 0 && g <= 255) && (b >= 0 && b <= 255)) {
                            let c = { rgb: [r, g, b], class_name: 'truecolor' };
                            if (is_foreground)
                                this.fg = c;
                            else
                                this.bg = c;
                        }
                    }
                }
            }
        }
    }
    transform_to_html(fragment) {
        let txt = fragment.text;
        if (txt.length === 0)
            return txt;
        txt = this.escape_txt_for_html(txt);
        if (!fragment.bold && !fragment.italic && !fragment.faint && !fragment.underline && fragment.fg === null && fragment.bg === null)
            return txt;
        const styles = [];
        const classes = [];
        const fg = fragment.fg;
        const bg = fragment.bg;
        if (!this._use_classes) {
            if (fragment.bold)
                styles.push(this._boldStyle);
            if (fragment.faint)
                styles.push(this._faintStyle);
            if (fragment.italic)
                styles.push(this._italicStyle);
            if (fragment.underline)
                styles.push(this._underlineStyle);
            if (fg)
                styles.push(`color:rgb(${fg.rgb.join(',')})`);
            if (bg)
                styles.push(`background-color:rgb(${bg.rgb})`);
        }
        else {
            if (fragment.bold)
                classes.push("bold");
            if (fragment.faint)
                classes.push("faint");
            if (fragment.italic)
                classes.push("italic");
            if (fragment.underline)
                classes.push("underline");
            if (fg) {
                if (fg.class_name !== 'truecolor') {
                    classes.push(`${fg.class_name}-fg`);
                }
                else {
                    styles.push(`color:rgb(${fg.rgb.join(',')})`);
                }
            }
            if (bg) {
                if (bg.class_name !== 'truecolor') {
                    classes.push(`${bg.class_name}-bg`);
                }
                else {
                    styles.push(`background-color:rgb(${bg.rgb.join(',')})`);
                }
            }
        }
        let class_string = '';
        let style_string = '';
        if (classes.length)
            class_string = ` class="${classes.join(' ')}"`;
        if (styles.length)
            style_string = ` style="${styles.join(';')}"`;
        return `<span${style_string}${class_string}>${txt}</span>`;
    }
    ;
    process_hyperlink(pkt) {
        let parts = pkt.url.split(':');
        if (parts.length < 1)
            return '';
        if (!this._url_allowlist[parts[0]])
            return '';
        let result = `<a href="${this.escape_txt_for_html(pkt.url)}">${this.escape_txt_for_html(pkt.text)}</a>`;
        return result;
    }
}
function rgx(tmplObj, ...subst) {
    let regexText = tmplObj.raw[0];
    let wsrgx = /^\s+|\s+\n|\s*#[\s\S]*?\n|\n/gm;
    let txt2 = regexText.replace(wsrgx, '');
    return new RegExp(txt2);
}
function rgxG(tmplObj, ...subst) {
    let regexText = tmplObj.raw[0];
    let wsrgx = /^\s+|\s+\n|\s*#[\s\S]*?\n|\n/gm;
    let txt2 = regexText.replace(wsrgx, '');
    return new RegExp(txt2, 'g');
}
var templateObject_1, templateObject_2, templateObject_3;
